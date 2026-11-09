//! SCH-06 `monitor_school_absenteeism`.
//!
//! Reads daily absences by school from a local file (a line list with one row
//! per absent student-day, or daily counts), sorts absence reasons into
//! influenza-like, gastrointestinal, rash, and other illness, and runs CDC's
//! EARS C1-C3 aberration detection (the Phase 3 engine,
//! [`phn_epi::aberration`]) on each school's daily series (and optionally
//! each grade, classroom, or team). Signals prompt a call to the local board
//! of health under 105 CMR 300.134 and DPH's school norovirus guidance; the
//! tool never decides that an outbreak exists. Exact daily counts go to an
//! XLSX in the output folder; only suppressed counts are returned.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use phn_epi::aberration::{EarsMethod, EarsParams, ears};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::localdata;
use crate::md;
use crate::school::content::{self, ABSENCE, CitedOut, outs};

/// Arguments for `monitor_school_absenteeism`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AbsenteeismArgs {
    /// CSV or XLSX in /data: either one row per absent student-day (Date,
    /// School, Reason; names and IDs are never read) or daily counts (Date,
    /// School, Count, and optionally Reason, or columns per syndrome such as
    /// ILI and GI, plus Enrollment).
    pub path: String,
    /// Sheet name for an XLSX file.
    pub sheet: Option<String>,
    /// Column overrides (found automatically when omitted).
    pub date_column: Option<String>,
    pub school_column: Option<String>,
    pub reason_column: Option<String>,
    pub count_column: Option<String>,
    pub enrollment_column: Option<String>,
    /// Also look for clusters within each grade, classroom, or team (a
    /// category column with at most 20 values), as in DPH's example of a
    /// sports team with vomiting and diarrhea.
    pub group_column: Option<String>,
    /// Syndromes to monitor: ili, gi, rash, other_illness, all_illness,
    /// total (default: ili, gi, all_illness).
    pub syndromes: Option<Vec<String>>,
    /// Last day to evaluate (default: the last date in the file).
    pub end_date: Option<String>,
    /// School days to show and to check for signals (default 5, at most 30).
    pub report_days: Option<usize>,
}

/// A recent day for one school (counts suppressed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DayRow {
    pub date: String,
    /// Syndrome id -> count ("<n" when suppressed).
    pub counts: BTreeMap<String, String>,
    /// Percent of enrollment absent for illness, when enrollment is given.
    pub illness_pct: Option<f64>,
    /// Methods that signaled on this day, by syndrome ("gi: C1, C2").
    pub signals: Vec<String>,
}

/// One school (or school and group).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SchoolSummary {
    pub school: String,
    pub group: Option<String>,
    pub school_days: usize,
    pub recent: Vec<DayRow>,
    /// Number of signals in the report window.
    pub signals: usize,
}

/// One signal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SignalOut {
    pub school: String,
    pub group: Option<String>,
    pub syndrome: String,
    pub date: String,
    pub methods: Vec<String>,
    /// Observed count ("<n" when suppressed).
    pub observed: String,
    /// Mean of the C1 baseline (the 7 school days before).
    pub baseline_mean: f64,
}

/// Result of `monitor_school_absenteeism`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AbsenteeismResult {
    pub file: String,
    pub first_date: String,
    pub last_date: String,
    pub school_days: usize,
    pub syndromes: Vec<String>,
    pub schools: Vec<SchoolSummary>,
    pub signals: Vec<SignalOut>,
    /// Absence reasons by syndrome across the file (counts of rows), for checking the sorting.
    pub reason_mapping: BTreeMap<String, usize>,
    pub when_to_notify: Vec<CitedOut>,
    pub control_measures: Vec<CitedOut>,
    pub examples: Vec<CitedOut>,
    pub method: String,
    pub output_file: String,
    pub suppression_threshold: u32,
}

const ALL_ILLNESS: &str = "all_illness";
const TOTAL: &str = "total";

fn syndrome_ids() -> Vec<String> {
    let mut v: Vec<String> = ABSENCE.syndromes.iter().map(|s| s.id.clone()).collect();
    v.push(ALL_ILLNESS.into());
    v.push(TOTAL.into());
    v
}

fn syndrome_label(id: &str) -> String {
    match id {
        ALL_ILLNESS => "all illness".into(),
        TOTAL => "all absences".into(),
        _ => ABSENCE
            .syndromes
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| id.to_string()),
    }
}

/// Sort a free-text absence reason into a syndrome (None = not illness).
pub fn classify(reason: &str) -> Option<&'static str> {
    let r = phn_core::text::normalize(reason);
    if r.is_empty() {
        return None;
    }
    // GI first (a "stomach flu" is gastrointestinal), then ILI, rash, other.
    for id in ["gi", "ili", "rash", "other_illness"] {
        if let Some(s) = ABSENCE.syndromes.iter().find(|s| s.id == id)
            && s.keywords.iter().any(|k| {
                let kn = phn_core::text::normalize(k);
                if kn.len() <= 3 {
                    r.split_whitespace().any(|w| w == kn)
                } else {
                    r.contains(&kn)
                }
            })
        {
            return Some(match id {
                "gi" => "gi",
                "ili" => "ili",
                "rash" => "rash",
                _ => "other_illness",
            });
        }
    }
    None
}

fn find(headers: &[String], given: Option<&str>, names: &[&str]) -> Result<Option<usize>> {
    if let Some(g) = given {
        let n = phn_core::text::normalize(g);
        return headers
            .iter()
            .position(|h| phn_core::text::normalize(h) == n)
            .map(Some)
            .ok_or_else(|| {
                PhnError::InvalidInput(format!(
                    "no column named '{g}'. Columns found: {}",
                    headers.join(", ")
                ))
            });
    }
    Ok(headers.iter().position(|h| {
        let n = phn_core::text::normalize(h);
        names.iter().any(|x| n == *x)
    }))
}

#[derive(Debug, Clone, Default)]
struct Day {
    counts: BTreeMap<String, f64>,
    enrollment: Option<f64>,
}

fn number(s: &str) -> Option<f64> {
    s.trim()
        .replace(',', "")
        .parse::<f64>()
        .ok()
        .filter(|x| x.is_finite() && *x >= 0.0)
}

fn median(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        (s[n / 2 - 1] + s[n / 2]) / 2.0
    }
}

/// Monitor illness absenteeism for unusual increases.
pub fn monitor_school_absenteeism(
    ctx: &Context,
    args: AbsenteeismArgs,
) -> Result<Envelope<AbsenteeismResult>> {
    if args.path.trim().is_empty() {
        return Err(PhnError::InvalidInput(
            "give the absence file (CSV or XLSX in /data)".into(),
        ));
    }
    let lt = localdata::load(ctx, &args.path, args.sheet.as_deref())?;
    let h = lt.table.headers.clone();
    let date_col = find(
        &h,
        args.date_column.as_deref(),
        &["date", "absence date", "day", "attendance date"],
    )?
    .ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "the file needs a 'Date' column. Columns found: {}",
            h.join(", ")
        ))
    })?;
    let school_col = find(
        &h,
        args.school_column.as_deref(),
        &["school", "school name", "building", "site"],
    )?;
    let reason_col = find(
        &h,
        args.reason_column.as_deref(),
        &[
            "reason",
            "absence reason",
            "illness",
            "symptoms",
            "symptom",
            "reason for absence",
            "category",
        ],
    )?;
    let count_col = find(
        &h,
        args.count_column.as_deref(),
        &[
            "count",
            "absent",
            "absences",
            "number absent",
            "n",
            "total absent",
            "students absent",
        ],
    )?;
    let enroll_col = find(
        &h,
        args.enrollment_column.as_deref(),
        &["enrollment", "enrolled", "enrollment count"],
    )?;
    // Wide format: numeric columns named for syndromes.
    let wide: Vec<(usize, &'static str)> = h
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != count_col && Some(*i) != reason_col)
        .filter_map(|(i, x)| {
            let n = phn_core::text::normalize(x);
            let id: &'static str = match n.as_str() {
                "ili" | "influenza like illness" | "respiratory" | "flu like" => "ili",
                "gi" | "gastrointestinal" | "vomiting diarrhea" | "vomiting or diarrhea" => "gi",
                "rash" => "rash",
                "other illness" | "illness other" => "other_illness",
                _ => return None,
            };
            Some((i, id))
        })
        .collect();
    if reason_col.is_none() && wide.is_empty() && count_col.is_none() {
        return Err(PhnError::InvalidInput(
            "the file needs a 'Reason' column (one row per absence), a 'Count' column, or \
             syndrome columns such as 'ILI' and 'GI'"
                .into(),
        ));
    }
    let group_col = match args.group_column.as_deref() {
        Some(g) => {
            let i = lt.column(g)?;
            lt.categorical(i)?;
            Some(i)
        }
        None => None,
    };
    let wanted: Vec<String> = match &args.syndromes {
        Some(v) if !v.is_empty() => {
            let ids = syndrome_ids();
            v.iter()
                .map(|s| {
                    let n = phn_core::text::normalize(s).replace(' ', "_");
                    let n = match n.as_str() {
                        "respiratory" | "flu" | "influenza" => "ili".to_string(),
                        "gastrointestinal" | "stomach" | "norovirus" => "gi".to_string(),
                        "illness" | "all" => ALL_ILLNESS.to_string(),
                        "absences" | "all_absences" => TOTAL.to_string(),
                        other => other.to_string(),
                    };
                    if ids.contains(&n) {
                        Ok(n)
                    } else {
                        Err(PhnError::InvalidInput(format!(
                            "syndrome '{s}' is not recognized; use {}",
                            ids.join(", ")
                        )))
                    }
                })
                .collect::<Result<Vec<_>>>()?
        }
        _ => vec!["ili".into(), "gi".into(), ALL_ILLNESS.into()],
    };
    let end = match &args.end_date {
        Some(s) => Some(crate::vax::dates::require_date("end_date", s)?),
        None => None,
    };
    let report_days = args.report_days.unwrap_or(5).clamp(1, 30);

    // ---- Aggregate: (school, group) -> date -> counts
    let mut series: BTreeMap<(String, Option<String>), BTreeMap<NaiveDate, Day>> = BTreeMap::new();
    let mut all_dates: BTreeSet<NaiveDate> = BTreeSet::new();
    let mut mapping: BTreeMap<String, usize> = BTreeMap::new();
    let mut bad_rows = 0usize;
    for row in &lt.table.rows {
        let get = |i: Option<usize>| i.and_then(|i| row.get(i)).map(|s| s.trim()).unwrap_or("");
        let Some(d) = crate::vax::dates::parse_date(get(Some(date_col))) else {
            if !row.iter().all(|c| c.trim().is_empty()) {
                bad_rows += 1;
            }
            continue;
        };
        if end.is_some_and(|e| d > e) {
            continue;
        }
        all_dates.insert(d);
        let school = school_col
            .map(|_| get(school_col).to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "All schools".into());
        let mut keys = vec![(school.clone(), None)];
        if let Some(g) = group_col {
            let gv = get(Some(g)).to_string();
            if !gv.is_empty() {
                keys.push((school.clone(), Some(gv)));
            }
        }
        let count = match count_col {
            Some(_) => number(get(count_col)),
            None => Some(1.0),
        };
        let enrollment = number(get(enroll_col));
        for k in keys {
            let day = series.entry(k.clone()).or_default().entry(d).or_default();
            if k.1.is_none() && enrollment.is_some() {
                day.enrollment = enrollment;
            }
            if !wide.is_empty() {
                for (i, id) in &wide {
                    if let Some(v) = number(get(Some(*i))) {
                        *day.counts.entry((*id).to_string()).or_default() += v;
                        *day.counts.entry(ALL_ILLNESS.to_string()).or_default() += v;
                    }
                }
                if let Some(c) = count {
                    *day.counts.entry(TOTAL.to_string()).or_default() += c;
                }
                continue;
            }
            let Some(c) = count else {
                continue;
            };
            *day.counts.entry(TOTAL.to_string()).or_default() += c;
            if reason_col.is_some() {
                let r = get(reason_col);
                let syn = classify(r);
                if k.1.is_none() {
                    *mapping
                        .entry(
                            syn.map(syndrome_label)
                                .unwrap_or_else(|| "not illness or unspecified".into()),
                        )
                        .or_default() += 1;
                }
                if let Some(s) = syn {
                    *day.counts.entry(s.to_string()).or_default() += c;
                    *day.counts.entry(ALL_ILLNESS.to_string()).or_default() += c;
                }
            }
        }
    }
    if all_dates.len() < 8 {
        return Err(PhnError::InvalidInput(format!(
            "the file has {} school day(s); EARS needs at least 8 (C3 needs 12). Use a longer period.",
            all_dates.len()
        )));
    }
    let days: Vec<NaiveDate> = all_dates.iter().copied().collect();
    let report_from = days.len().saturating_sub(report_days);
    let threshold = ctx.suppression_threshold();
    let shown = |v: f64| -> String {
        let n = v.round() as u64;
        if n > 0 && n < u64::from(threshold) {
            format!("<{threshold}")
        } else {
            n.to_string()
        }
    };
    let mut schools = Vec::new();
    let mut signals = Vec::new();
    let mut detail_rows = Vec::new();
    let mut signal_rows = Vec::new();
    for ((school, group), by_date) in &series {
        let mut recent: Vec<DayRow> = Vec::new();
        let mut per_day_signals: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        let mut n_signals = 0usize;
        for syn in &wanted {
            let counts: Vec<f64> = days
                .iter()
                .map(|d| {
                    by_date
                        .get(d)
                        .and_then(|x| x.counts.get(syn))
                        .copied()
                        .unwrap_or(0.0)
                })
                .collect();
            let sparse = median(&counts) < 2.0;
            let mut fired: BTreeMap<usize, (Vec<String>, f64)> = BTreeMap::new();
            for m in [EarsMethod::C1, EarsMethod::C2, EarsMethod::C3] {
                let mut p = EarsParams::classic(m);
                if sparse {
                    p.min_sigma = 0.5;
                }
                let Ok(points) = ears(&counts, m, &p) else {
                    continue;
                };
                for pt in points
                    .iter()
                    .filter(|pt| pt.alarm && pt.index >= report_from)
                {
                    let e = fired.entry(pt.index).or_insert((Vec::new(), 0.0));
                    e.0.push(format!("{m:?}"));
                    if m == EarsMethod::C1 {
                        e.1 = pt.baseline_mean;
                    }
                }
            }
            for (idx, (methods, mean)) in fired {
                n_signals += 1;
                per_day_signals.entry(idx).or_default().push(format!(
                    "{}: {}",
                    syn,
                    methods.join(", ")
                ));
                let base = if mean == 0.0 {
                    let lo = idx.saturating_sub(7);
                    counts[lo..idx].iter().sum::<f64>() / (idx - lo).max(1) as f64
                } else {
                    mean
                };
                signals.push(SignalOut {
                    school: school.clone(),
                    group: group.clone(),
                    syndrome: syndrome_label(syn),
                    date: days[idx].to_string(),
                    methods: methods.clone(),
                    observed: shown(counts[idx]),
                    baseline_mean: (base * 10.0).round() / 10.0,
                });
                signal_rows.push(vec![
                    school.clone(),
                    group.clone().unwrap_or_default(),
                    syndrome_label(syn),
                    days[idx].to_string(),
                    format!("{}", counts[idx]),
                    format!("{:.1}", base),
                    methods.join(", "),
                ]);
            }
        }
        for (i, d) in days.iter().enumerate() {
            let day = by_date.get(d);
            let mut row = vec![
                school.clone(),
                group.clone().unwrap_or_default(),
                d.to_string(),
            ];
            for syn in &wanted {
                row.push(format!(
                    "{}",
                    day.and_then(|x| x.counts.get(syn)).copied().unwrap_or(0.0)
                ));
            }
            row.push(
                per_day_signals
                    .get(&i)
                    .map(|v| v.join("; "))
                    .unwrap_or_default(),
            );
            detail_rows.push(row);
            if i >= report_from {
                let illness = day
                    .and_then(|x| x.counts.get(ALL_ILLNESS))
                    .copied()
                    .unwrap_or(0.0);
                recent.push(DayRow {
                    date: d.to_string(),
                    counts: wanted
                        .iter()
                        .map(|s| {
                            (
                                s.clone(),
                                shown(day.and_then(|x| x.counts.get(s)).copied().unwrap_or(0.0)),
                            )
                        })
                        .collect(),
                    illness_pct: day
                        .and_then(|x| x.enrollment)
                        .filter(|e| *e > 0.0)
                        .map(|e| ((illness / e) * 1000.0).round() / 10.0),
                    signals: per_day_signals.get(&i).cloned().unwrap_or_default(),
                });
            }
        }
        schools.push(SchoolSummary {
            school: school.clone(),
            group: group.clone(),
            school_days: days.len(),
            recent,
            signals: n_signals,
        });
    }

    // ---- Output workbook (exact counts; no student identifiers).
    let mut dh = vec![
        "School".to_string(),
        "Group".to_string(),
        "Date".to_string(),
    ];
    dh.extend(wanted.iter().map(|s| syndrome_label(s)));
    dh.push("Signals".into());
    let mut map_rows: Vec<Vec<String>> = mapping
        .iter()
        .map(|(k, v)| vec![k.clone(), v.to_string()])
        .collect();
    map_rows.sort();
    let doc = Document {
        title: "School illness absenteeism".into(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks: vec![
            Block::Paragraph(vec![Inline::plain(format!(
                "Daily absences by school from {}, {} to {}; EARS C1-C3 signals in the last {report_days} school day(s). A signal is a statistical prompt to review the data and call the local board of health, not an outbreak determination.",
                lt.label,
                days[0],
                days[days.len() - 1]
            ))]),
            Block::Table(TableBlock {
                caption: Some("Signals".into()),
                headers: [
                    "School",
                    "Group",
                    "Syndrome",
                    "Date",
                    "Observed",
                    "Baseline mean",
                    "Methods",
                ]
                .map(String::from)
                .to_vec(),
                rows: signal_rows,
            }),
            Block::Table(TableBlock {
                caption: Some("Daily counts".into()),
                headers: dh,
                rows: detail_rows,
            }),
            Block::Table(TableBlock {
                caption: Some("How reasons were sorted".into()),
                headers: vec!["Syndrome".into(), "Rows".into()],
                rows: map_rows,
            }),
        ],
    };
    let sandbox = ctx.sandbox()?;
    let out =
        sandbox.new_output_file(&format!("school_absenteeism_{}.xlsx", days[days.len() - 1]))?;
    std::fs::write(&out, phn_docs::xlsx::to_xlsx(&doc)?)?;
    let output_file = sandbox.display_path(&out);

    // ---- Guidance
    let c = &*ABSENCE;
    let when_to_notify = outs(&c.notification, &c.sources);
    let flagged: BTreeSet<&str> = signals
        .iter()
        .filter_map(|s| {
            c.syndromes
                .iter()
                .find(|x| x.name == s.syndrome)
                .map(|x| x.id.as_str())
        })
        .collect();
    let control_measures: Vec<CitedOut> = c
        .control_measures
        .iter()
        .filter(|m| flagged.is_empty() || flagged.contains(m.syndrome.as_str()))
        .map(|m| CitedOut {
            text: m.text.trim().to_string(),
            citation: content::citation(&c.sources, &m.source, &m.section),
            url: c.sources.get(&m.source).map(|s| s.url.clone()),
            confidence: None,
        })
        .collect();
    let examples = outs(&c.examples, &c.sources);

    // ---- Markdown
    let mut s = format!(
        "**School illness absenteeism** — {} school day(s), {} to {}\n\n",
        days.len(),
        days[0],
        days[days.len() - 1]
    );
    if signals.is_empty() {
        s.push_str(&format!(
            "No EARS signals in the last {report_days} school day(s) for {}.\n\n",
            wanted
                .iter()
                .map(|w| syndrome_label(w))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    } else {
        s.push_str(&format!(
            "**{} signal(s) in the last {report_days} school day(s)**\n\n",
            signals.len()
        ));
        let rows: Vec<Vec<String>> = signals
            .iter()
            .map(|x| {
                vec![
                    x.school.clone(),
                    x.group.clone().unwrap_or_else(|| "—".into()),
                    x.syndrome.clone(),
                    x.date.clone(),
                    x.observed.clone(),
                    format!("{:.1}", x.baseline_mean),
                    x.methods.join(", "),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "School",
                "Group",
                "Syndrome",
                "Date",
                "Observed",
                "Usual (7-day mean)",
                "EARS",
            ],
            &rows,
        ));
        s.push_str(
            "\n**Next step:** review these students' symptoms with the school nurse records and \
             call your local board of health. Under 105 CMR 300.134 a school supervisor must report \
             any suspected cluster or outbreak immediately; DPH defines an outbreak as cases clearly \
             in excess of what is usual for the setting and season, with no fixed number.",
        );
        if flagged.contains("gi") {
            s.push_str(
                " For vomiting and diarrhea, use DPH's online Gastrointestinal Illness Child Care \
                 & K-12 School Cluster Reporting Form; the board of health or DPH will call back.",
            );
        }
        s.push('\n');
    }
    s.push_str("\n**Recent days by school** (counts 1 to ");
    s.push_str(&format!("{} shown as `<{threshold}`)\n\n", threshold - 1));
    let mut headers = vec![
        "School".to_string(),
        "Group".to_string(),
        "Date".to_string(),
    ];
    headers.extend(wanted.iter().map(|w| syndrome_label(w)));
    headers.push("Illness % of enrolled".into());
    let mut rows = Vec::new();
    for sc in schools
        .iter()
        .filter(|x| x.group.is_none() || x.signals > 0)
    {
        for d in &sc.recent {
            let mut r = vec![
                sc.school.clone(),
                sc.group.clone().unwrap_or_else(|| "—".into()),
                d.date.clone(),
            ];
            r.extend(
                wanted
                    .iter()
                    .map(|w| d.counts.get(w).cloned().unwrap_or_default()),
            );
            r.push(
                d.illness_pct
                    .map(|p| format!("{p:.1}"))
                    .unwrap_or_else(|| "—".into()),
            );
            rows.push(r);
        }
    }
    let hdr: Vec<&str> = headers.iter().map(String::as_str).collect();
    s.push_str(&md::table(&hdr, &rows));
    if !mapping.is_empty() {
        s.push_str(&format!(
            "\nAbsence reasons were sorted as: {}. Check the sorting in the output file.\n",
            mapping
                .iter()
                .map(|(k, v)| format!("{k} {v}"))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    s.push_str(&format!(
        "\nDaily counts and signals are in `{output_file}`.\n"
    ));

    let mut caveats = vec![
        "EARS C1-C3 flag days that are high compared with the previous 7 school days (C2 and C3 \
         skip 2 days). They are screening aids with known false alarms, and they miss slow rises \
         and clusters hidden inside a whole school's total; look within grades, classrooms, and \
         teams (group_column) too."
            .to_string(),
        "School days are the dates present in the file; days with no absences must still appear \
         (as a row with zero, or any row for that date in another school) or they are skipped."
            .into(),
        "DPH publishes no numeric absenteeism threshold for calling the board of health; decide \
         with the board of health (105 CMR 300.134)."
            .into(),
    ];
    if bad_rows > 0 {
        caveats.push(format!(
            "{bad_rows} row(s) had no valid date and were skipped."
        ));
    }
    if reason_col.is_some() && wide.is_empty() {
        caveats.push(
            "Absence reasons are sorted into syndromes by keywords (tool defaults listed in the \
             content file); parent-reported reasons are rough. Review the 'How reasons were \
             sorted' sheet."
                .into(),
        );
    }
    caveats.extend(content::review(&[content::ABSENTEEISM]));
    let mut sources = vec![Source::local_file(lt.label.clone())];
    sources.extend(content::sources_for(
        &c.sources,
        [
            "cmr300",
            "dph-norovirus-schools",
            "cdc-fluview",
            "hutwagner-2003",
        ],
    ));
    let identifiers = lt.identifier_categories();
    Ok(Envelope::new(
        s,
        AbsenteeismResult {
            file: lt.label.clone(),
            first_date: days[0].to_string(),
            last_date: days[days.len() - 1].to_string(),
            school_days: days.len(),
            syndromes: wanted,
            schools,
            signals,
            reason_mapping: mapping,
            when_to_notify,
            control_measures,
            examples,
            method: "CDC EARS C1, C2, C3 (7-day baseline; C2/C3 with a 2-day guard band; thresholds 3, 3, 2; baseline SD floor 0.5 when the median daily count is below 2), computed per school and syndrome by phn_epi::aberration.".into(),
            output_file: output_file.clone(),
            suppression_threshold: threshold,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        suppression_applied: true,
        suppression_threshold: Some(threshold),
        identifiers_detected: identifiers,
        files_written: vec![output_file],
        notes: vec![
            "Student names and IDs in the file are never read; daily counts from 1 to the threshold are shown as '<n' here and exactly in the output file.".into(),
        ],
    })
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    fn ctx_with(file: &str, body: &str) -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(dir.path().join("data").join(file), body).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn reasons_sort_into_syndromes() {
        assert_eq!(classify("Stomach bug - vomiting"), Some("gi"));
        assert_eq!(classify("fever and cough"), Some("ili"));
        assert_eq!(classify("Flu"), Some("ili"));
        assert_eq!(classify("chickenpox"), Some("rash"));
        assert_eq!(classify("sick"), Some("other_illness"));
        assert_eq!(classify("family vacation"), None);
    }

    #[test]
    fn gi_spike_signals_and_counts_are_suppressed() {
        // 20 school days, two schools; school B has a GI spike on the last two days.
        let mut body = String::from("Date,School,Count,Reason,Enrollment\n");
        let start = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let mut d = start;
        let mut n = 0;
        while n < 20 {
            if !matches!(
                chrono::Datelike::weekday(&d),
                chrono::Weekday::Sat | chrono::Weekday::Sun
            ) {
                for (school, base) in [("Alder School", 6), ("Birch School", 7)] {
                    let ili = base / 2 + (n % 3);
                    let gi = if school == "Birch School" && n >= 18 {
                        14
                    } else {
                        1 + (n % 2)
                    };
                    writeln!(body, "{d},{school},{ili},fever cough,300").unwrap();
                    writeln!(body, "{d},{school},{gi},vomiting,300").unwrap();
                    writeln!(body, "{d},{school},3,appointment,300").unwrap();
                }
                n += 1;
            }
            d = d.succ_opt().unwrap();
        }
        let (_dir, c) = ctx_with("a.csv", &body);
        let e = monitor_school_absenteeism(
            &c,
            AbsenteeismArgs {
                path: "a.csv".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let r = &e.data;
        assert_eq!(r.school_days, 20);
        assert!(
            r.signals
                .iter()
                .any(|s| s.school == "Birch School" && s.syndrome.starts_with("Gastro"))
        );
        assert!(
            !r.signals
                .iter()
                .any(|s| s.school == "Alder School" && s.syndrome.starts_with("Gastro"))
        );
        // Small counts are hidden.
        assert!(
            r.schools[0]
                .recent
                .iter()
                .any(|d| d.counts["gi"].starts_with('<'))
        );
        assert!(e.summary.contains("300.134"));
        assert!(
            r.control_measures
                .iter()
                .any(|m| m.text.contains("norovirus"))
        );
        assert!(r.recent_pct_present());
    }

    impl AbsenteeismResult {
        fn recent_pct_present(&self) -> bool {
            self.schools
                .iter()
                .all(|s| s.recent.iter().all(|d| d.illness_pct.is_some()))
        }
    }

    #[test]
    fn too_few_days_is_a_tool_error() {
        let (_d, c) = ctx_with(
            "b.csv",
            "Date,School,Reason\n2026-10-01,A,flu\n2026-10-02,A,flu\n",
        );
        let e = monitor_school_absenteeism(
            &c,
            AbsenteeismArgs {
                path: "b.csv".into(),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("at least 8"));
    }
}
