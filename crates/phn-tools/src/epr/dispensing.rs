//! EPR-02 `model_dispensing_site_throughput`: staffing, queues, and time to
//! reach a population at emergency dispensing sites (PODs), with each
//! station modeled as an M/M/c queue (`phn_epi::queueing`).

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use phn_epi::queueing::{self, MmcMeasures};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self, Assumption, CitedOut, Dispensing, StationDef};
use crate::context::Context;
use crate::md;

/// A station override.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct StationInput {
    /// Station id from the model (e.g. "forms", "dispensing", "vaccination") or a new station name.
    pub station: String,
    /// Average minutes one staff member spends per client (measure this in your exercises).
    pub minutes_per_client: Option<f64>,
    /// Share of clients who visit this station (0-1), e.g. 0.15 for medical consultation.
    pub fraction_of_clients: Option<f64>,
    /// Staff working this station at once. Give it to evaluate a staffing plan; omit to size it.
    pub staff: Option<u32>,
}

/// Arguments for `model_dispensing_site_throughput`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct DispensingArgs {
    /// People who need the countermeasure (for example the population of your town).
    pub population: u64,
    /// "oral_prophylaxis" (default; pills such as antibiotics) or "vaccination".
    pub model: Option<String>,
    /// Number of dispensing sites open at the same time (default 1).
    pub sites: Option<u32>,
    /// Total hours each site will be open to reach everyone (for example 48 for two
    /// round-the-clock days, or 36 for three 12-hour days). Needed unless arrivals_per_hour is given.
    pub operating_hours: Option<f64>,
    /// Instead of operating_hours: the expected arrivals per hour at each site.
    pub arrivals_per_hour: Option<f64>,
    /// Regimens picked up per visit (head-of-household pickup; default 1 = everyone comes in person).
    pub regimens_per_visit: Option<f64>,
    /// Override station times, shares, or staffing.
    pub stations: Vec<StationInput>,
    /// Highest share of time staff may be busy when sizing (default 0.85).
    pub max_utilization: Option<f64>,
    /// Longest acceptable average wait at a station when sizing, in minutes (default 10).
    pub target_wait_minutes: Option<f64>,
    /// Hours a site is open each day (default 12), for shifts and calendar days.
    pub hours_open_per_day: Option<f64>,
    /// Shift length in hours (default 6).
    pub shift_hours: Option<f64>,
    /// Command, security, flow, and other support staff per shift (not modeled; default 0).
    pub support_staff_per_shift: Option<u32>,
    /// Busiest-hour arrivals relative to the average (default 1.25), to test surge.
    pub surge_factor: Option<f64>,
    /// Write a staffing worksheet (XLSX) to /data/output (default false).
    pub worksheet: Option<bool>,
}

/// Queue measures for one station.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StationResult {
    pub station: String,
    pub label: String,
    pub minutes_per_client: f64,
    pub fraction_of_clients: f64,
    /// "tool_default", "source", or "your_input".
    pub basis: String,
    /// Clients arriving at this station per hour.
    pub arrivals_per_hour: f64,
    pub staff: u32,
    /// "sized by the tool" or "your staffing".
    pub staffing_basis: String,
    /// Share of time staff are busy (0-1).
    pub utilization: f64,
    /// Probability a client waits (Erlang C).
    pub prob_wait: Option<f64>,
    pub mean_wait_minutes: Option<f64>,
    pub mean_line_length: Option<f64>,
    /// Probability the wait exceeds the target wait.
    pub prob_wait_over_target: Option<f64>,
    /// Most clients per hour the station can serve (all sites' clients counted at this station).
    pub capacity_per_hour: f64,
    /// Utilization at the busiest hour (surge); above 1 means the line keeps growing.
    pub surge_utilization: f64,
    /// False when arrivals exceed capacity (the line grows without limit).
    pub stable: bool,
}

/// Result of `model_dispensing_site_throughput`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DispensingResult {
    pub model: String,
    pub population: u64,
    pub sites: u32,
    pub regimens_per_visit: f64,
    /// Visits needed (population ÷ regimens per visit).
    pub visits_needed: f64,
    /// Average arrivals per hour at each site.
    pub arrivals_per_hour_per_site: f64,
    pub stations: Vec<StationResult>,
    /// The station that limits throughput.
    pub bottleneck: String,
    /// Most clients per hour one site can serve.
    pub site_capacity_per_hour: f64,
    /// Regimens dispensed per hour at one site at the modeled arrival rate (or capacity).
    pub regimens_per_hour_per_site: f64,
    /// Hours of operation needed to reach everyone with all sites at the modeled rate.
    pub operating_hours_needed: f64,
    /// Calendar days at the configured hours per day.
    pub days_needed: f64,
    /// True when every station is stable at the average arrival rate.
    pub feasible: bool,
    /// Average minutes a client spends at the site (waits, service, observation).
    pub mean_minutes_in_site: Option<f64>,
    /// Average number of clients inside a site (Little's law), for space planning.
    pub mean_clients_in_site: Option<f64>,
    pub station_staff_per_shift: u32,
    pub support_staff_per_shift: u32,
    pub staff_per_shift_per_site: u32,
    pub shifts_per_day: u32,
    /// Staff shifts to fill across all sites for the whole operation.
    pub total_staff_shifts: u32,
    pub assumptions: Vec<AssumptionOut>,
    pub definitions: Vec<CitedOut>,
    pub activation_elements: Vec<String>,
    pub access_considerations: Vec<CitedOut>,
    pub support_roles: Vec<String>,
    pub method: String,
    pub worksheet_file: Option<String>,
}

/// A planning assumption as used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AssumptionOut {
    pub label: String,
    pub value: f64,
    pub unit: String,
    /// "tool_default", "source", "computed", or "your_input".
    pub basis: String,
    pub note: String,
}

const METHOD: &str = "Each station is an M/M/c queue (Poisson arrivals, exponential service \
    times, c staff, first come first served, steady state). Station arrivals = site arrivals × \
    share of clients visiting the station; service rate = 60 ÷ minutes per client. Erlang C gives \
    the probability of waiting; mean wait Wq = C/(cμ − λ); line length Lq = λWq; time at the \
    site = Σ share × (Wq + service time) + observation; clients inside = arrivals × time \
    (Little's law). When sizing, each station gets the fewest staff with utilization at or below \
    the maximum and mean wait at or below the target. Site capacity = min over stations of \
    staff × service rate ÷ share. Hours needed = visits ÷ (sites × min(arrivals, capacity)). \
    Reference: Gross, Shortle, Thompson, and Harris, Fundamentals of Queueing Theory, 4th ed., \
    sec. 2.3.";

fn positive(name: &str, v: Option<f64>, max: f64) -> Result<Option<f64>> {
    match v {
        Some(x) if !(x.is_finite() && x > 0.0 && x <= max) => Err(PhnError::InvalidInput(format!(
            "{name} must be a number greater than 0 and at most {max}"
        ))),
        other => Ok(other),
    }
}

fn assumption_out(a: &Assumption, over: Option<f64>) -> (f64, AssumptionOut) {
    match over {
        Some(v) => (
            v,
            AssumptionOut {
                label: a.label.clone(),
                value: v,
                unit: a.unit.clone(),
                basis: "your_input".into(),
                note: format!("You set this (tool default {} {}).", a.value, a.unit),
            },
        ),
        None => (
            a.value,
            AssumptionOut {
                label: a.label.clone(),
                value: a.value,
                unit: a.unit.clone(),
                basis: a.basis.clone(),
                note: a.note.clone(),
            },
        ),
    }
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Model dispensing-site throughput, queues, and staffing.
pub fn model_dispensing_site_throughput(
    ctx: &Context,
    args: DispensingArgs,
) -> Result<Envelope<DispensingResult>> {
    let c: Dispensing = content::load(content::DISPENSING)?;
    if args.population == 0 {
        return Err(PhnError::InvalidInput(
            "population must be at least 1".into(),
        ));
    }
    let model_id = args
        .model
        .as_deref()
        .map(|m| m.trim().to_ascii_lowercase().replace([' ', '-'], "_"))
        .unwrap_or_else(|| "oral_prophylaxis".into());
    let model_id = match model_id.as_str() {
        "oral" | "pills" | "antibiotics" | "prophylaxis" | "oral_prophylaxis" | "medication" => {
            "oral_prophylaxis"
        }
        "vaccine" | "vaccines" | "vaccination" | "mass_vaccination" => "vaccination",
        other => {
            return Err(PhnError::InvalidInput(format!(
                "model must be oral_prophylaxis or vaccination (got '{other}')"
            )));
        }
    };
    let model = c
        .models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| PhnError::Internal(format!("model {model_id} missing")))?;
    let sites = args.sites.unwrap_or(1);
    if !(1..=500).contains(&sites) {
        return Err(PhnError::InvalidInput("sites must be 1-500".into()));
    }
    let regimens = positive("regimens_per_visit", args.regimens_per_visit, 20.0)?.unwrap_or(1.0);
    let op_hours = positive("operating_hours", args.operating_hours, 24.0 * 60.0)?;
    let arrivals_in = positive("arrivals_per_hour", args.arrivals_per_hour, 100_000.0)?;
    let mut assumptions = Vec::new();
    let (max_util, a) = assumption_out(
        c.default_value("max_utilization")?,
        match args.max_utilization {
            Some(u) if !(u.is_finite() && u > 0.0 && u < 1.0) => {
                return Err(PhnError::InvalidInput(
                    "max_utilization must be between 0 and 1 (for example 0.85)".into(),
                ));
            }
            other => other,
        },
    );
    assumptions.push(a);
    let (target_wait, a) = assumption_out(
        c.default_value("target_wait_minutes")?,
        positive("target_wait_minutes", args.target_wait_minutes, 600.0)?,
    );
    assumptions.push(a);
    let (hours_per_day, a) = assumption_out(
        c.default_value("hours_open_per_day")?,
        positive("hours_open_per_day", args.hours_open_per_day, 24.0)?,
    );
    assumptions.push(a);
    let (shift_hours, a) = assumption_out(
        c.default_value("shift_hours")?,
        positive("shift_hours", args.shift_hours, 24.0)?,
    );
    assumptions.push(a);
    let (surge, a) = assumption_out(
        c.default_value("surge_factor")?,
        match args.surge_factor {
            Some(s) if !(s.is_finite() && (1.0..=5.0).contains(&s)) => {
                return Err(PhnError::InvalidInput(
                    "surge_factor must be between 1 and 5".into(),
                ));
            }
            other => other,
        },
    );
    assumptions.push(a);

    let visits = args.population as f64 / regimens;
    let lambda = match (arrivals_in, op_hours) {
        (Some(a), _) => a,
        (None, Some(h)) => visits / (sites as f64 * h),
        (None, None) => {
            return Err(PhnError::InvalidInput(
                "give operating_hours (the hours each site will be open to reach everyone, e.g. \
                 48) or arrivals_per_hour (expected arrivals at each site)"
                    .into(),
            ));
        }
    };
    if lambda > 100_000.0 {
        return Err(PhnError::InvalidInput(format!(
            "that is {} arrivals per hour per site; add sites or operating hours",
            md::num(lambda, 0)
        )));
    }

    // Stations: model defaults with overrides, plus new stations.
    let mut defs: Vec<(StationDef, Option<u32>, bool)> = model
        .stations
        .iter()
        .map(|s| (s.clone(), None, false))
        .collect();
    for o in &args.stations {
        let key = phn_core::text::normalize(&o.station);
        if key.is_empty() {
            return Err(PhnError::InvalidInput(
                "each station override needs a station name".into(),
            ));
        }
        if let Some(m) = o.minutes_per_client
            && !(m.is_finite() && m > 0.0 && m <= 240.0)
        {
            return Err(PhnError::InvalidInput(format!(
                "minutes_per_client for {} must be between 0 and 240",
                o.station
            )));
        }
        if let Some(f) = o.fraction_of_clients
            && !(f.is_finite() && f > 0.0 && f <= 1.0)
        {
            return Err(PhnError::InvalidInput(format!(
                "fraction_of_clients for {} must be between 0 and 1",
                o.station
            )));
        }
        if o.staff == Some(0) {
            return Err(PhnError::InvalidInput(format!(
                "staff for {} must be at least 1 (remove the station by leaving it out of your plan)",
                o.station
            )));
        }
        let found = defs.iter_mut().find(|(d, _, _)| {
            phn_core::text::normalize(&d.id) == key
                || phn_core::text::normalize(&d.label).starts_with(&key)
        });
        match found {
            Some((d, staff, changed)) => {
                if let Some(m) = o.minutes_per_client {
                    d.minutes_per_client = m;
                    *changed = true;
                }
                if let Some(f) = o.fraction_of_clients {
                    d.fraction_of_clients = f;
                    *changed = true;
                }
                if o.staff.is_some() {
                    *staff = o.staff;
                }
            }
            None => {
                let m = o.minutes_per_client.ok_or_else(|| {
                    PhnError::InvalidInput(format!(
                        "'{}' is not a station in the {} model ({}); give minutes_per_client to add it",
                        o.station,
                        model.id,
                        model
                            .stations
                            .iter()
                            .map(|s| s.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                })?;
                defs.push((
                    StationDef {
                        id: key.replace(' ', "_"),
                        label: o.station.trim().to_string(),
                        minutes_per_client: m,
                        fraction_of_clients: o.fraction_of_clients.unwrap_or(1.0),
                        basis: "your_input".into(),
                        note: "Added by you.".into(),
                    },
                    o.staff,
                    true,
                ));
            }
        }
    }

    // Queue each station.
    let mut stations = Vec::new();
    let mut caveats: Vec<String> = Vec::new();
    for (d, staff_in, changed) in &defs {
        let mu = 60.0 / d.minutes_per_client;
        let lam = lambda * d.fraction_of_clients;
        let (staff, staffing_basis) = match staff_in {
            Some(s) => (*s, "your staffing"),
            None => {
                let s = queueing::min_servers(lam, mu, max_util, Some(target_wait / 60.0), 5_000)
                    .ok_or_else(|| {
                    PhnError::InvalidInput(format!(
                        "{} would need more than 5,000 staff; add sites or operating hours",
                        d.label
                    ))
                })?;
                (s, "sized by the tool")
            }
        };
        let m: Option<MmcMeasures> = queueing::mmc(staff, lam, mu);
        let cap = staff as f64 * mu;
        stations.push(StationResult {
            station: d.id.clone(),
            label: d.label.clone(),
            minutes_per_client: d.minutes_per_client,
            fraction_of_clients: d.fraction_of_clients,
            basis: if *changed {
                "your_input".into()
            } else {
                d.basis.clone()
            },
            arrivals_per_hour: r2(lam),
            staff,
            staffing_basis: staffing_basis.into(),
            utilization: r2(lam / cap),
            prob_wait: m.map(|m| r2(m.prob_wait)),
            mean_wait_minutes: m.map(|m| (m.mean_wait * 600.0).round() / 10.0),
            mean_line_length: m.map(|m| (m.mean_queue * 10.0).round() / 10.0),
            prob_wait_over_target: m
                .map(|m| r2(queueing::prob_wait_exceeds(&m, target_wait / 60.0))),
            capacity_per_hour: r2(cap / d.fraction_of_clients),
            surge_utilization: r2(lam * surge / cap),
            stable: m.is_some(),
        });
    }
    let bottleneck = stations
        .iter()
        .min_by(|a, b| a.capacity_per_hour.total_cmp(&b.capacity_per_hour))
        .expect("at least one station");
    let site_capacity = bottleneck.capacity_per_hour;
    let bottleneck_label = bottleneck.label.clone();
    let feasible = stations.iter().all(|s| s.stable);
    let served_per_hour = lambda.min(site_capacity);
    let hours_needed = visits / (sites as f64 * served_per_hour);
    let days_needed = hours_needed / hours_per_day;
    let mean_minutes = feasible.then(|| {
        stations
            .iter()
            .map(|s| {
                s.fraction_of_clients * (s.mean_wait_minutes.unwrap_or(0.0) + s.minutes_per_client)
            })
            .sum::<f64>()
            + model.observation_minutes
    });
    let in_site = mean_minutes.map(|m| lambda * m / 60.0);
    let station_staff: u32 = stations.iter().map(|s| s.staff).sum();
    let support = args.support_staff_per_shift.unwrap_or(0);
    let per_shift = station_staff + support;
    let shifts_per_day = (hours_per_day / shift_hours).ceil() as u32;
    let total_shifts =
        (per_shift as f64 * sites as f64 * (hours_needed / shift_hours).ceil()).round() as u32;

    if !feasible {
        let over: Vec<String> = stations
            .iter()
            .filter(|s| !s.stable)
            .map(|s| {
                format!(
                    "{} ({} staff serve at most {}/hour)",
                    s.label,
                    s.staff,
                    md::num(s.capacity_per_hour * s.fraction_of_clients, 0)
                )
            })
            .collect();
        caveats.push(format!(
            "NOT FEASIBLE at {} arrivals per hour: the line grows without limit at {}. Add staff at \
             those stations, add sites, or extend operating hours.",
            md::num(lambda, 0),
            over.join("; ")
        ));
    }
    let surging: Vec<&str> = stations
        .iter()
        .filter(|s| s.stable && s.surge_utilization >= 1.0)
        .map(|s| s.label.as_str())
        .collect();
    if !surging.is_empty() {
        caveats.push(format!(
            "In the busiest hour ({surge}× average arrivals) these stations cannot keep up and lines \
             will build until arrivals slow: {}. Plan flex staff.",
            surging.join("; ")
        ));
    }
    if let (Some(h), true) = (op_hours, arrivals_in.is_none()) {
        assumptions.push(AssumptionOut {
            label: "Operating hours to reach everyone".into(),
            value: h,
            unit: "hours".into(),
            basis: "your_input".into(),
            note: format!(
                "Average arrivals per site = {} visits ÷ ({sites} site(s) × {h} hours).",
                md::num(visits, 0)
            ),
        });
    }

    // Worksheet.
    let mut files_written = Vec::new();
    let mut worksheet_file = None;
    if args.worksheet.unwrap_or(false) {
        let rows: Vec<Vec<String>> = stations
            .iter()
            .map(|s| {
                vec![
                    s.label.clone(),
                    s.minutes_per_client.to_string(),
                    s.fraction_of_clients.to_string(),
                    s.arrivals_per_hour.to_string(),
                    s.staff.to_string(),
                    s.utilization.to_string(),
                    s.mean_wait_minutes
                        .map(|w| w.to_string())
                        .unwrap_or_else(|| "unstable".into()),
                    s.capacity_per_hour.to_string(),
                    s.basis.clone(),
                ]
            })
            .collect();
        let arows: Vec<Vec<String>> = assumptions
            .iter()
            .map(|a| {
                vec![
                    a.label.clone(),
                    a.value.to_string(),
                    a.unit.clone(),
                    a.basis.clone(),
                    a.note.clone(),
                ]
            })
            .collect();
        let doc = Document {
            title: "Dispensing site staffing and throughput".into(),
            subtitle: Some(model.label.clone()),
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks: vec![
                Block::Paragraph(vec![Inline::plain(format!(
                    "{} people, {} regimen(s) per visit, {sites} site(s), {} arrivals per hour per site. Replace tool defaults with times measured in your exercises.",
                    args.population,
                    regimens,
                    md::num(lambda, 1)
                ))]),
                Block::Table(TableBlock {
                    caption: Some("Stations".into()),
                    headers: [
                        "Station",
                        "Minutes per client",
                        "Share of clients",
                        "Arrivals per hour",
                        "Staff",
                        "Utilization",
                        "Mean wait (min)",
                        "Capacity per hour",
                        "Basis",
                    ]
                    .map(String::from)
                    .to_vec(),
                    rows,
                }),
                Block::Table(TableBlock {
                    caption: Some("Assumptions".into()),
                    headers: ["Assumption", "Value", "Unit", "Basis", "Note"]
                        .map(String::from)
                        .to_vec(),
                    rows: arows,
                }),
            ],
        };
        let bytes = phn_docs::xlsx::to_xlsx(&doc)?;
        let sandbox = ctx.sandbox()?;
        let path = sandbox.new_output_file("dispensing_site_staffing.xlsx")?;
        std::fs::write(&path, bytes)?;
        let disp = sandbox.display_path(&path);
        files_written.push(disp.clone());
        worksheet_file = Some(disp);
    }

    // Summary.
    let mut s = format!(
        "**Dispensing site model — {}**\n\n{} people{}, {sites} site(s): **{} arrivals per hour per site**.\n\n",
        model.label,
        md::num(args.population as f64, 0),
        if regimens != 1.0 {
            format!(
                " ({regimens} regimens per visit → {} visits)",
                md::num(visits, 0)
            )
        } else {
            String::new()
        },
        md::num(lambda, 1)
    );
    let rows: Vec<Vec<String>> = stations
        .iter()
        .map(|st| {
            vec![
                st.label.clone(),
                format!(
                    "{} min{}",
                    st.minutes_per_client,
                    if st.fraction_of_clients < 1.0 {
                        format!(" ({:.0}% of clients)", st.fraction_of_clients * 100.0)
                    } else {
                        String::new()
                    }
                ),
                format!(
                    "{}{}",
                    st.staff,
                    if st.staffing_basis == "your staffing" {
                        " (yours)"
                    } else {
                        ""
                    }
                ),
                format!("{:.0}%", st.utilization * 100.0),
                st.mean_wait_minutes
                    .map(|w| format!("{w} min"))
                    .unwrap_or_else(|| "grows without limit".into()),
                st.mean_line_length
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "—".into()),
                md::num(st.capacity_per_hour, 0),
            ]
        })
        .collect();
    s.push_str(&md::table(
        &[
            "Station",
            "Service time",
            "Staff",
            "Busy",
            "Mean wait",
            "People in line",
            "Capacity/hour",
        ],
        &rows,
    ));
    s.push_str(&format!(
        "\n**Bottleneck:** {bottleneck_label} (site capacity {} clients/hour). **Staff per site per shift:** {per_shift} ({station_staff} at stations{}). Shifts per day: {shifts_per_day} of {shift_hours} hours.\n\n",
        md::num(site_capacity, 0),
        if support > 0 { format!(" + {support} support") } else { "; add command, security, flow, and AFN support staff".into() }
    ));
    s.push_str(&format!(
        "**Time to reach everyone:** {} operating hours ({:.1} days at {hours_per_day} hours/day){}. Total staff shifts across all sites: {}.\n",
        md::num(hours_needed, 1),
        days_needed,
        if let Some(h) = op_hours {
            if hours_needed <= h + 1e-9 { format!(" — meets your {h}-hour window") } else { format!(" — DOES NOT meet your {h}-hour window") }
        } else {
            String::new()
        },
        md::num(total_shifts as f64, 0)
    ));
    if let (Some(m), Some(n)) = (mean_minutes, in_site) {
        s.push_str(&format!(
            "Average time at the site: {m:.0} minutes; about {} people inside at once (plan space and parking).\n",
            md::num(n, 0)
        ));
    }
    s.push_str("\n**Assumptions** (replace tool defaults with your exercise data)\n\n");
    let arows: Vec<Vec<String>> = stations
        .iter()
        .map(|st| {
            vec![
                format!("{} time", st.label),
                format!("{} min", st.minutes_per_client),
                st.basis.replace('_', " "),
            ]
        })
        .chain(assumptions.iter().map(|a| {
            vec![
                a.label.clone(),
                format!("{} {}", a.value, a.unit),
                a.basis.replace('_', " "),
            ]
        }))
        .collect();
    s.push_str(&md::table(&["Assumption", "Value", "Basis"], &arows));
    s.push_str(&format!(
        "\n**Activation checklist (CDC PHEP Capability 8):** {}.\n",
        c.activation_elements.items.join("; ")
    ));
    if let Some(f) = &worksheet_file {
        s.push_str(&format!("\nStaffing worksheet written to `{f}`.\n"));
    }

    caveats.push(
        "Station times and shares marked 'tool default' are planning placeholders with no \
         published source; the results are only as good as these inputs. Time each station in \
         a drill and enter your own minutes_per_client."
            .into(),
    );
    caveats.push(
        "The M/M/c model assumes random (Poisson) arrivals and variable service times in steady \
         state. Real arrivals come in waves (opening hour, shift changes, media announcements), \
         so lines can be longer than the averages shown; see the surge check."
            .into(),
    );
    if regimens > 1.0 {
        caveats.push(
            "Head-of-household pickup: station times are per visit; screening and dispensing \
             usually take longer per visit when one person collects several regimens."
                .into(),
        );
    }
    caveats.extend(content::review_caveats(&[content::DISPENSING]));
    let mut sources = vec![Source::computed(
        "M/M/c (Erlang C) queueing model",
        "Gross, Shortle, Thompson & Harris, Fundamentals of Queueing Theory, 4th ed. (2008)",
    )];
    sources.extend(content::sources_for(
        c.sources.keys().map(String::as_str),
        &c.sources,
    ));

    Ok(Envelope::new(
        s,
        DispensingResult {
            model: model.id.clone(),
            population: args.population,
            sites,
            regimens_per_visit: regimens,
            visits_needed: visits.round(),
            arrivals_per_hour_per_site: r2(lambda),
            stations,
            bottleneck: bottleneck_label,
            site_capacity_per_hour: r2(site_capacity),
            regimens_per_hour_per_site: r2(served_per_hour * regimens),
            operating_hours_needed: (hours_needed * 10.0).round() / 10.0,
            days_needed: (days_needed * 10.0).round() / 10.0,
            feasible,
            mean_minutes_in_site: mean_minutes.map(|m| m.round()),
            mean_clients_in_site: in_site.map(|n| n.round()),
            station_staff_per_shift: station_staff,
            support_staff_per_shift: support,
            staff_per_shift_per_site: per_shift,
            shifts_per_day,
            total_staff_shifts: total_shifts,
            assumptions,
            definitions: content::cite_all(&c.definitions, &c.sources),
            activation_elements: c.activation_elements.items.clone(),
            access_considerations: content::cite_all(&c.access_considerations, &c.sources),
            support_roles: c.support_roles.items.clone(),
            method: METHOD.into(),
            worksheet_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        files_written,
        ..Privacy::none()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn sizes_each_station_and_reports_time_to_reach_everyone() {
        let (_d, c) = ctx();
        let env = model_dispensing_site_throughput(
            &c,
            DispensingArgs {
                population: 48_000,
                sites: Some(2),
                operating_hours: Some(48.0),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &env.data;
        // 48,000 visits / (2 sites × 48 h) = 500 arrivals per hour per site.
        assert!((d.arrivals_per_hour_per_site - 500.0).abs() < 1e-9);
        assert!(d.feasible);
        for s in &d.stations {
            assert!(s.utilization <= 0.85 + 1e-9, "{}", s.station);
            assert!(s.mean_wait_minutes.unwrap() <= 10.0, "{}", s.station);
        }
        // Forms: 500/h × 2 min = 16.7 busy staff; at most 85% busy → 20.
        let forms = d.stations.iter().find(|s| s.station == "forms").unwrap();
        assert_eq!(forms.staff, 20);
        // Arrivals are below capacity, so everyone is reached in the 48 hours.
        assert!((d.operating_hours_needed - 48.0).abs() < 1e-9);
        assert_eq!(d.days_needed, 4.0);
        assert!(env.summary.contains("meets your 48-hour window"));
        assert!(d.mean_clients_in_site.unwrap() > 0.0);
        assert_eq!(
            d.staff_per_shift_per_site,
            d.stations.iter().map(|s| s.staff).sum::<u32>()
        );
    }

    #[test]
    fn evaluates_a_staffing_plan_and_flags_the_bottleneck() {
        let (_d, c) = ctx();
        let env = model_dispensing_site_throughput(
            &c,
            DispensingArgs {
                population: 10_000,
                model: Some("vaccination".into()),
                arrivals_per_hour: Some(120.0),
                stations: vec![
                    StationInput {
                        station: "check_in".into(),
                        staff: Some(8),
                        ..Default::default()
                    },
                    StationInput {
                        station: "screening".into(),
                        staff: Some(10),
                        ..Default::default()
                    },
                    StationInput {
                        station: "vaccination".into(),
                        staff: Some(9),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        )
        .unwrap();
        let d = &env.data;
        // Vaccination: 9 staff × 12/hour = 108/hour < 120 arrivals: unstable.
        assert!(!d.feasible);
        assert_eq!(d.bottleneck, "Vaccination");
        assert!((d.site_capacity_per_hour - 108.0).abs() < 1e-9);
        // 10,000 visits at 108/hour.
        assert!((d.operating_hours_needed - 92.6).abs() < 0.05);
        assert!(env.caveats[0].contains("NOT FEASIBLE"));
        // Observation (15 min) is not a queue but counts toward time at the site when stable.
        assert!(d.mean_minutes_in_site.is_none());
    }

    #[test]
    fn bad_inputs_are_errors() {
        let (_d, c) = ctx();
        let run = |a: DispensingArgs| model_dispensing_site_throughput(&c, a);
        assert!(
            run(DispensingArgs {
                population: 100,
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            run(DispensingArgs {
                population: 100,
                operating_hours: Some(10.0),
                model: Some("drones".into()),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            run(DispensingArgs {
                population: 100,
                operating_hours: Some(10.0),
                max_utilization: Some(1.2),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            run(DispensingArgs {
                population: 100,
                operating_hours: Some(10.0),
                stations: vec![StationInput {
                    station: "decon".into(),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .unwrap_err()
            .to_string()
            .contains("minutes_per_client")
        );
    }
}
