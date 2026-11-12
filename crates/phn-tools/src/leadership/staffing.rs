//! LEAD-02 `model_staffing_needs`: estimates public health nurse full-time equivalents (FTE) from
//! service volumes and time standards the nurse supplies, with an adjustable productive-hours
//! calculation, scenarios (baseline, growth, added services), and population or caseload per
//! nurse. No staffing benchmark is bundled (none could be read from a published source); a
//! benchmark the nurse types in is compared and labelled as hers.
//!
//! Inputs are typed or read from a CSV/XLSX of service volumes through `localdata` (identifier
//! columns are refused; only the service, volume, and minutes columns are read). The workbook
//! has live formulas; a DOCX summary goes to /data/output. Decision support only.

use std::collections::BTreeSet;

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_local::phi;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::finance_content::{self as fc, StaffingContent, padded};
use super::staffing_output;
use crate::context::Context;
use crate::localdata;
use crate::school::templates::DocFormat;

const MAX_SERVICES: usize = 20;
const MAX_SCENARIOS: usize = 8;

// ---------------------------------------------------------------- arguments

/// One service with its volume and time standard.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct StaffingServiceArg {
    /// The service, for example "Home visits" or "Immunization clinic doses". Do not type a
    /// person's name.
    pub name: String,
    /// Units per year (visits, doses, inspections, cases investigated).
    pub annual_volume: f64,
    /// Nurse minutes per unit, including the work you want counted with it (preparation,
    /// documentation, travel). This is your time standard; the tool does not supply one.
    pub minutes_per_unit: f64,
}

/// A scenario to compare with the baseline.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ScenarioArg {
    /// Scenario name, for example "10% growth" or "Add a diabetes prevention program".
    pub name: String,
    /// Percent change applied to every baseline service's volume (for example 10 or -5).
    pub volume_growth_percent: Option<f64>,
    /// Services added in this scenario, with their own volumes and minutes (not grown).
    pub added_services: Option<Vec<StaffingServiceArg>>,
}

/// A staffing benchmark you supply.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct BenchmarkArg {
    /// What the benchmark is, for example "Agency target".
    pub label: String,
    /// Where it comes from (document, year, page). The tool does not verify it.
    pub source: String,
    /// Residents per nurse FTE under the benchmark.
    pub population_per_fte: Option<f64>,
    /// Clients (active cases) per nurse FTE under the benchmark.
    pub clients_per_fte: Option<f64>,
}

/// Arguments for `model_staffing_needs`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct StaffingArgs {
    /// Services typed in (up to 20). Give these or `volumes_path`.
    pub services: Option<Vec<StaffingServiceArg>>,
    /// CSV or XLSX in /data with one row per service: service name, annual volume, minutes per
    /// unit. Only those three columns are read.
    pub volumes_path: Option<String>,
    /// Sheet name for an XLSX file (default: the first sheet).
    pub sheet: Option<String>,
    /// Service-name column (default: Service, Program, Activity).
    pub service_column: Option<String>,
    /// Annual volume column (default: Volume, Annual Volume, Units).
    pub volume_column: Option<String>,
    /// Minutes-per-unit column (default: Minutes, Minutes per Unit).
    pub minutes_column: Option<String>,
    /// Paid hours per full-time employee per year (default 2,080 = 40 x 52).
    pub paid_hours_per_year: Option<f64>,
    /// Paid holiday hours (default 88, a placeholder).
    pub holiday_hours: Option<f64>,
    /// Vacation hours taken per year (default 120, a placeholder).
    pub vacation_hours: Option<f64>,
    /// Sick and personal hours per year (default 64, a placeholder).
    pub sick_personal_hours: Option<f64>,
    /// Required training hours per year (default 24, a placeholder).
    pub training_hours: Option<f64>,
    /// Share of the remaining hours that is not direct service: meetings, supervision,
    /// documentation not in the minutes, travel (percent; default 20, a placeholder).
    pub non_service_percent: Option<f64>,
    /// Current nurse FTE on staff, to show the gap.
    pub current_fte: Option<f64>,
    /// Population served, to show residents per nurse FTE.
    pub population: Option<f64>,
    /// Active clients or cases, to show caseload per nurse FTE.
    pub clients: Option<f64>,
    /// Scenarios to compare with the baseline (up to 8).
    pub scenarios: Option<Vec<ScenarioArg>>,
    /// A benchmark you want to compare with (yours; none is bundled).
    pub benchmark: Option<BenchmarkArg>,
    /// Round FTE up to this step (default 0.1, for example 0.5 for half-time positions).
    pub rounding_step_fte: Option<f64>,
    /// Program or department name for the report title.
    pub program_name: Option<String>,
    /// Write the workbook with live formulas to /data/output (default true).
    pub write_workbook: Option<bool>,
    /// Write the summary report to /data/output (default true).
    pub write_report: Option<bool>,
    /// docx (default) or pdf, for the report.
    pub format: Option<DocFormat>,
}

// ---------------------------------------------------------------- results

/// One productive-hours input and where its value came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HoursInput {
    pub label: String,
    pub hours: f64,
    /// "you gave this" or "tool default (placeholder, no published source)" or "arithmetic".
    pub basis: String,
}

/// The productive-hours calculation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProductiveHours {
    pub inputs: Vec<HoursInput>,
    /// Paid hours minus holiday, vacation, sick and personal, and training hours.
    pub available_hours: f64,
    pub non_service_percent: f64,
    pub non_service_basis: String,
    /// Direct-service hours one FTE has in a year.
    pub productive_hours_per_fte: f64,
}

/// Hours for one service in the baseline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceHours {
    pub service: String,
    pub annual_volume: f64,
    pub minutes_per_unit: f64,
    pub hours: f64,
    pub percent_of_hours: Option<f64>,
}

/// One scenario's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScenarioResult {
    pub name: String,
    pub description: String,
    pub volume_growth_percent: f64,
    pub added_hours: f64,
    pub total_hours: f64,
    pub change_in_hours_vs_baseline: f64,
    /// Hours divided by productive hours per FTE.
    pub fte_needed: f64,
    /// FTE rounded up to the rounding step.
    pub fte_rounded_up: f64,
    /// Rounded FTE minus current FTE (positive means more staff are needed), when current_fte given.
    pub gap_vs_current_fte: Option<f64>,
    /// Population divided by FTE needed, when population given.
    pub residents_per_fte_needed: Option<f64>,
    /// Clients divided by FTE needed, when clients given.
    pub clients_per_fte_needed: Option<f64>,
}

/// Residents and clients per nurse at current staffing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CurrentStaffing {
    pub current_fte: f64,
    pub residents_per_fte: Option<f64>,
    pub clients_per_fte: Option<f64>,
    /// Baseline hours of work divided by the hours the current staff have.
    pub baseline_workload_percent_of_capacity: f64,
}

/// A comparison with the nurse's own benchmark.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BenchmarkComparison {
    pub label: String,
    pub source_given_by_you: String,
    pub population_per_fte: Option<f64>,
    pub clients_per_fte: Option<f64>,
    /// FTE the benchmark implies for the population (population / benchmark).
    pub fte_implied_by_population: Option<f64>,
    pub fte_implied_by_clients: Option<f64>,
    pub note: String,
}

/// A benchmark that was looked for and not bundled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NotBundled {
    pub item: String,
    pub detail: String,
}

/// Result of `model_staffing_needs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StaffingResult {
    pub program: Option<String>,
    pub productive_hours: ProductiveHours,
    pub services: Vec<ServiceHours>,
    pub baseline_hours: f64,
    pub scenarios: Vec<ScenarioResult>,
    pub current: Option<CurrentStaffing>,
    pub benchmark_comparison: Option<BenchmarkComparison>,
    /// Always "none": no published public health nurse staffing benchmark is bundled.
    pub bundled_benchmarks: String,
    /// Benchmarks and time standards that were looked for and not used, and why.
    pub benchmarks_not_bundled: Vec<NotBundled>,
    pub rounding_step_fte: f64,
    pub workbook_file: Option<String>,
    pub report_file: Option<String>,
}

// ---------------------------------------------------------------- calculation

pub(crate) fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Round FTE up to a step (the same rule as the workbook's `ROUNDUP(ROUND(x/step,6),0)*step`).
pub(crate) fn round_up(x: f64, step: f64) -> f64 {
    let q = ((x / step) * 1e6).round() / 1e6;
    q.ceil() * step
}

fn clean(what: &str, s: &str) -> Result<String> {
    let t: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        return Err(PhnError::InvalidInput(format!("{what} is empty")));
    }
    if t.chars().count() > 120 {
        return Err(PhnError::InvalidInput(format!(
            "{what} is longer than 120 characters"
        )));
    }
    if !phi::scan_text(&t).is_empty() {
        return Err(PhnError::Privacy(format!(
            "{what} appears to contain an identifier; use the service or program name"
        )));
    }
    Ok(t)
}

fn check_service(s: &StaffingServiceArg, what: &str) -> Result<(String, f64, f64)> {
    let name = clean(&format!("{what} name"), &s.name)?;
    if !(s.annual_volume.is_finite() && (0.0..=1.0e9).contains(&s.annual_volume)) {
        return Err(PhnError::InvalidInput(format!(
            "{what} '{name}': annual_volume must be between 0 and 1,000,000,000"
        )));
    }
    if !(s.minutes_per_unit.is_finite() && s.minutes_per_unit > 0.0 && s.minutes_per_unit <= 1440.0) {
        return Err(PhnError::InvalidInput(format!(
            "{what} '{name}': minutes_per_unit must be more than 0 and at most 1,440"
        )));
    }
    Ok((name, s.annual_volume, s.minutes_per_unit))
}

fn hours_of(volume: f64, minutes: f64) -> f64 {
    volume * minutes / 60.0
}

/// Services from a CSV/XLSX of volumes.
fn read_services(
    ctx: &Context,
    args: &StaffingArgs,
    path: &str,
) -> Result<(Vec<StaffingServiceArg>, Source, Vec<String>, Vec<String>)> {
    let lt = localdata::load(ctx, path, args.sheet.as_deref())?;
    if lt.table.truncated {
        return Err(PhnError::InvalidInput(
            "the file has more rows than this tool reads".into(),
        ));
    }
    let find = |explicit: &Option<String>, aliases: &[&str], what: &str| -> Result<usize> {
        if let Some(n) = explicit.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            return lt.column(n);
        }
        for a in aliases {
            let want = padded(a);
            for (i, h) in lt.table.headers.iter().enumerate() {
                if padded(h) == want && lt.column(h).is_ok() {
                    return Ok(i);
                }
            }
        }
        Err(PhnError::InvalidInput(format!(
            "no {what} column found; name one with the matching *_column argument. Columns you \
             can use: {}",
            lt.table
                .headers
                .iter()
                .filter(|h| lt.column(h).is_ok())
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )))
    };
    let si = find(&args.service_column, &["service", "program", "activity", "service name"], "service")?;
    let vi = find(
        &args.volume_column,
        &["annual volume", "volume", "units", "units per year", "annual units"],
        "volume",
    )?;
    let mi = find(
        &args.minutes_column,
        &["minutes per unit", "minutes", "minutes per visit", "time standard"],
        "minutes",
    )?;
    if si == vi || si == mi || vi == mi {
        return Err(PhnError::InvalidInput(
            "the service, volume, and minutes columns must be three different columns".into(),
        ));
    }
    let names = lt.categorical(si)?;
    let (vols, _) = lt.numeric(vi);
    let (mins, _) = lt.numeric(mi);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut skipped = 0usize;
    for r in 0..lt.table.rows.len() {
        let (Some(n), Some(v), Some(m)) = (names[r].clone(), vols[r], mins[r]) else {
            skipped += 1;
            continue;
        };
        if !seen.insert(padded(&n)) {
            return Err(PhnError::InvalidInput(
                "a service appears on more than one row; give one row per service".into(),
            ));
        }
        out.push(StaffingServiceArg {
            name: n,
            annual_volume: v,
            minutes_per_unit: m,
        });
    }
    if out.is_empty() {
        return Err(PhnError::InvalidInput(
            "no row had a service name, a volume, and minutes".into(),
        ));
    }
    let mut notes = vec![
        "Only the service, volume, and minutes columns were read; no other column in the file was \
         read or returned."
            .to_string(),
    ];
    if skipped > 0 {
        notes.push(format!(
            "{skipped} row(s) were skipped because a service, volume, or minutes value was missing."
        ));
    }
    Ok((out, lt.source(), notes, lt.identifier_categories()))
}

#[allow(clippy::too_many_lines)]
pub fn model_staffing_needs(ctx: &Context, args: StaffingArgs) -> Result<Envelope<StaffingResult>> {
    let c = StaffingContent::load()?;
    let program = match &args.program_name {
        Some(p) if !p.trim().is_empty() => Some(clean("program_name", p)?),
        _ => None,
    };

    // Services.
    let mut sources: Vec<Source> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut identifier_categories: Vec<String> = Vec::new();
    let raw: Vec<StaffingServiceArg> = match (&args.services, &args.volumes_path) {
        (Some(_), Some(_)) => {
            return Err(PhnError::InvalidInput(
                "give either services or volumes_path, not both".into(),
            ));
        }
        (Some(s), None) if !s.is_empty() => s.clone(),
        (None, Some(p)) if !p.trim().is_empty() => {
            let (s, src, n, ids) = read_services(ctx, &args, p)?;
            sources.push(src);
            notes.extend(n);
            identifier_categories = ids;
            s
        }
        _ => {
            return Err(PhnError::InvalidInput(
                "give services (name, annual_volume, minutes_per_unit) or volumes_path".into(),
            ));
        }
    };
    if raw.len() > MAX_SERVICES {
        return Err(PhnError::InvalidInput(format!(
            "at most {MAX_SERVICES} services"
        )));
    }
    let mut services: Vec<(String, f64, f64)> = Vec::new();
    let mut seen = BTreeSet::new();
    for s in &raw {
        let t = check_service(s, "service")?;
        if !seen.insert(padded(&t.0)) {
            return Err(PhnError::InvalidInput(format!(
                "service '{}' is listed twice",
                t.0
            )));
        }
        services.push(t);
    }

    // Productive hours.
    let dv = |id: &str, given: Option<f64>| -> Result<(f64, bool)> {
        match given {
            Some(x) => Ok((x, true)),
            None => Ok((c.default_value(id)?, false)),
        }
    };
    let (paid, paid_given) = dv("paid_hours_per_year", args.paid_hours_per_year)?;
    let (holiday, holiday_given) = dv("holiday_hours", args.holiday_hours)?;
    let (vacation, vacation_given) = dv("vacation_hours", args.vacation_hours)?;
    let (sick, sick_given) = dv("sick_personal_hours", args.sick_personal_hours)?;
    let (training, training_given) = dv("training_hours", args.training_hours)?;
    let (non_service, ns_given) = dv("non_service_percent", args.non_service_percent)?;
    if !(paid.is_finite() && paid > 0.0 && paid <= 8784.0) {
        return Err(PhnError::InvalidInput(
            "paid_hours_per_year must be more than 0 and at most 8,784".into(),
        ));
    }
    for (n, v) in [
        ("holiday_hours", holiday),
        ("vacation_hours", vacation),
        ("sick_personal_hours", sick),
        ("training_hours", training),
    ] {
        if !(v.is_finite() && v >= 0.0) {
            return Err(PhnError::InvalidInput(format!("{n} must be 0 or more")));
        }
    }
    let available = paid - holiday - vacation - sick - training;
    if available <= 0.0 {
        return Err(PhnError::InvalidInput(
            "holiday, vacation, sick and personal, and training hours add up to all of the paid \
             hours; check them"
                .into(),
        ));
    }
    if !(non_service.is_finite() && (0.0..95.0).contains(&non_service)) {
        return Err(PhnError::InvalidInput(
            "non_service_percent must be at least 0 and less than 95".into(),
        ));
    }
    let productive = available * (1.0 - non_service / 100.0);
    let basis = |given: bool, computed: bool| {
        if given {
            "you gave this".to_string()
        } else if computed {
            "arithmetic: 40 hours x 52 weeks".to_string()
        } else {
            "tool default (placeholder, no published source)".to_string()
        }
    };
    let productive_hours = ProductiveHours {
        inputs: vec![
            HoursInput {
                label: "Paid hours per year".into(),
                hours: paid,
                basis: basis(paid_given, true),
            },
            HoursInput {
                label: "Holiday hours".into(),
                hours: holiday,
                basis: basis(holiday_given, false),
            },
            HoursInput {
                label: "Vacation hours".into(),
                hours: vacation,
                basis: basis(vacation_given, false),
            },
            HoursInput {
                label: "Sick and personal hours".into(),
                hours: sick,
                basis: basis(sick_given, false),
            },
            HoursInput {
                label: "Training hours".into(),
                hours: training,
                basis: basis(training_given, false),
            },
        ],
        available_hours: round2(available),
        non_service_percent: non_service,
        non_service_basis: basis(ns_given, false),
        productive_hours_per_fte: round2(productive),
    };

    // Optional context numbers.
    let positive = |what: &str, v: Option<f64>, max: f64| -> Result<Option<f64>> {
        match v {
            Some(x) if !(x.is_finite() && x > 0.0 && x <= max) => Err(PhnError::InvalidInput(
                format!("{what} must be more than 0 and at most {}", max as u64),
            )),
            other => Ok(other),
        }
    };
    let current_fte = positive("current_fte", args.current_fte, 10_000.0)?;
    let population = positive("population", args.population, 20_000_000.0)?;
    let clients = positive("clients", args.clients, 100_000_000.0)?;
    let step = args.rounding_step_fte.unwrap_or(0.1);
    if !(step.is_finite() && (0.01..=1.0).contains(&step)) {
        return Err(PhnError::InvalidInput(
            "rounding_step_fte must be between 0.01 and 1".into(),
        ));
    }

    // Baseline.
    let baseline_hours: f64 = services.iter().map(|s| hours_of(s.1, s.2)).sum();
    let service_rows: Vec<ServiceHours> = services
        .iter()
        .map(|s| {
            let h = hours_of(s.1, s.2);
            ServiceHours {
                service: s.0.clone(),
                annual_volume: s.1,
                minutes_per_unit: s.2,
                hours: round2(h),
                percent_of_hours: (baseline_hours > 0.0).then(|| round2(h / baseline_hours * 100.0)),
            }
        })
        .collect();

    let scenario = |name: String,
                    description: String,
                    growth: f64,
                    added: f64|
     -> ScenarioResult {
        let total = baseline_hours * (1.0 + growth / 100.0) + added;
        let fte = total / productive;
        let rounded = round_up(fte, step);
        ScenarioResult {
            name,
            description,
            volume_growth_percent: growth,
            added_hours: round2(added),
            total_hours: round2(total),
            change_in_hours_vs_baseline: round2(total - baseline_hours),
            fte_needed: round2(fte),
            fte_rounded_up: round2(rounded),
            gap_vs_current_fte: current_fte.map(|c| round2(rounded - c)),
            residents_per_fte_needed: population.filter(|_| fte > 0.0).map(|p| round2(p / fte)),
            clients_per_fte_needed: clients.filter(|_| fte > 0.0).map(|p| round2(p / fte)),
        }
    };
    let mut scenarios = vec![scenario(
        "Baseline".into(),
        "The services and volumes you gave, with no change.".into(),
        0.0,
        0.0,
    )];
    let mut added_by_scenario: Vec<Vec<(String, f64, f64)>> = vec![Vec::new()];
    let specs = args.scenarios.clone().unwrap_or_default();
    if specs.len() > MAX_SCENARIOS {
        return Err(PhnError::InvalidInput(format!(
            "at most {MAX_SCENARIOS} scenarios"
        )));
    }
    let mut seen_names = BTreeSet::from(["baseline".to_string()]);
    for sp in &specs {
        let name = clean("a scenario name", &sp.name)?;
        if !seen_names.insert(padded(&name).trim().to_string()) {
            return Err(PhnError::InvalidInput(format!(
                "scenario '{name}' repeats a name (\"Baseline\" is always included)"
            )));
        }
        let growth = sp.volume_growth_percent.unwrap_or(0.0);
        if !(growth.is_finite() && (-100.0..=1000.0).contains(&growth)) {
            return Err(PhnError::InvalidInput(format!(
                "scenario '{name}': volume_growth_percent must be between -100 and 1000"
            )));
        }
        let mut added = Vec::new();
        for a in sp.added_services.iter().flatten() {
            added.push(check_service(a, &format!("scenario '{name}' added service"))?);
        }
        if growth == 0.0 && added.is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "scenario '{name}' changes nothing: give volume_growth_percent or added_services"
            )));
        }
        let added_hours: f64 = added.iter().map(|a| hours_of(a.1, a.2)).sum();
        let mut desc = Vec::new();
        if growth != 0.0 {
            desc.push(format!("every service's volume {growth:+}%"));
        }
        if !added.is_empty() {
            desc.push(format!(
                "adds {}",
                added
                    .iter()
                    .map(|a| a.0.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        scenarios.push(scenario(name, desc.join("; "), growth, added_hours));
        added_by_scenario.push(added);
    }

    let current = current_fte.map(|f| CurrentStaffing {
        current_fte: f,
        residents_per_fte: population.map(|p| round2(p / f)),
        clients_per_fte: clients.map(|p| round2(p / f)),
        baseline_workload_percent_of_capacity: round2(baseline_hours / (f * productive) * 100.0),
    });

    // The nurse's own benchmark.
    let benchmark_comparison = match &args.benchmark {
        Some(b) => {
            let label = clean("benchmark label", &b.label)?;
            let src = clean("benchmark source", &b.source)?;
            for (n, v) in [("population_per_fte", b.population_per_fte), ("clients_per_fte", b.clients_per_fte)] {
                if let Some(x) = v
                    && !(x.is_finite() && x > 0.0)
                {
                    return Err(PhnError::InvalidInput(format!(
                        "benchmark {n} must be more than 0"
                    )));
                }
            }
            if b.population_per_fte.is_none() && b.clients_per_fte.is_none() {
                return Err(PhnError::InvalidInput(
                    "benchmark needs population_per_fte or clients_per_fte".into(),
                ));
            }
            if b.population_per_fte.is_some() && population.is_none()
                || b.clients_per_fte.is_some() && clients.is_none()
            {
                return Err(PhnError::InvalidInput(
                    "a benchmark per population needs population, and one per client needs clients"
                        .into(),
                ));
            }
            Some(BenchmarkComparison {
                label,
                source_given_by_you: src,
                population_per_fte: b.population_per_fte,
                clients_per_fte: b.clients_per_fte,
                fte_implied_by_population: b
                    .population_per_fte
                    .zip(population)
                    .map(|(b, p)| round2(p / b)),
                fte_implied_by_clients: b.clients_per_fte.zip(clients).map(|(b, p)| round2(p / b)),
                note: "A benchmark you supplied. This server did not verify it and bundles none."
                    .into(),
            })
        }
        None => None,
    };

    let mut result = StaffingResult {
        program: program.clone(),
        productive_hours,
        services: service_rows,
        baseline_hours: round2(baseline_hours),
        scenarios,
        current,
        benchmark_comparison,
        bundled_benchmarks: "none".into(),
        benchmarks_not_bundled: c
            .not_bundled
            .iter()
            .map(|n| NotBundled {
                item: n.item.clone(),
                detail: n.detail.clone(),
            })
            .collect(),
        rounding_step_fte: step,
        workbook_file: None,
        report_file: None,
    };

    // Files.
    let mut files_written: Vec<String> = Vec::new();
    let want_book = args.write_workbook.unwrap_or(true);
    let want_report = args.write_report.unwrap_or(true);
    if want_book || want_report {
        let inputs = staffing_output::OutputInputs {
            result: &result,
            added: &added_by_scenario,
            current_fte,
            population,
            clients,
            non_service,
            paid,
            holiday,
            vacation,
            sick,
            training,
        };
        let (dir, _) = crate::school::templates::output_folder(ctx, "staffing_model")?;
        let (book, report) = staffing_output::write_files(
            ctx,
            &dir,
            &inputs,
            want_book,
            want_report,
            args.format.unwrap_or_default(),
        )?;
        if let Some(f) = book {
            files_written.push(f.clone());
            result.workbook_file = Some(f);
        }
        if let Some(f) = report {
            files_written.push(f.clone());
            result.report_file = Some(f);
        }
    }

    let summary = staffing_output::summary(&result);
    let mut caveats = vec![
        "This is arithmetic on the volumes and time standards you supplied: FTE needed = hours of \
         work (volume x minutes per unit / 60, summed) divided by the productive hours one FTE has \
         in a year. It does not know whether your minutes per unit are realistic; a time study or \
         agency productivity data is a better basis than an estimate."
            .to_string(),
        "The productive-hours inputs marked 'tool default' are placeholders with no published \
         source. Replace them with your agency's holidays, leave, training requirements, and \
         non-service time (union contract, personnel policy)."
            .into(),
        "No public health nurse staffing benchmark is bundled: none could be read from a source \
         this server was able to fetch (see benchmarks_not_bundled). Comparisons with a benchmark \
         use only the one you supply, and the tool does not verify it. Ratios of residents or \
         clients per nurse describe workload, not quality or need."
            .into(),
        "Scenarios change volumes or add services, not minutes per unit or productive hours, and \
         they do not model hiring time, training new staff, or coverage for leave."
            .into(),
        "Staffing decisions also depend on skill mix, licensure, state requirements, and budget; \
         use this as one input to the agency's workforce planning, not as a determination."
            .into(),
    ];
    caveats.extend(fc::caveats(&[fc::STAFFING]));
    notes.push(
        "Service volumes, minutes, and scenario figures are the ones you supplied; no client or staff data is read."
            .into(),
    );
    if !files_written.is_empty() {
        notes.push("The workbook and report stay on your computer in the output folder.".into());
    }
    sources.push(Source::computed(
        "Productive hours and FTE arithmetic (hours of work divided by productive hours per FTE)",
        "Calculated by this server",
    ));
    let used = ["capacity_assessment", "saphe2_guidance"];
    sources.extend(crate::epr::content::sources_for(
        used.iter().copied(),
        &c.sources,
    ));
    let privacy = Privacy {
        row_level_data: false,
        suppression_applied: false,
        suppression_threshold: None,
        identifiers_detected: identifier_categories,
        files_written,
        notes,
    };
    Ok(Envelope::new(summary, result)
        .decision_support()
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_up_ignores_floating_point_noise() {
        assert!((round_up(2.3, 0.1) - 2.3).abs() < 1e-9);
        assert!((round_up(2.31, 0.1) - 2.4).abs() < 1e-9);
        assert!((round_up(1.0, 0.5) - 1.0).abs() < 1e-9);
        assert!((round_up(1.01, 0.5) - 1.5).abs() < 1e-9);
        assert!((round_up(0.1 + 0.2, 0.1) - 0.3).abs() < 1e-9);
    }

    #[test]
    fn hours_and_fte_match_hand_arithmetic() {
        // 1,200 visits at 45 minutes = 900 hours; productive hours default 1,427.2.
        assert!((hours_of(1200.0, 45.0) - 900.0).abs() < 1e-9);
        let c = StaffingContent::load().unwrap();
        let available = c.default_value("paid_hours_per_year").unwrap()
            - c.default_value("holiday_hours").unwrap()
            - c.default_value("vacation_hours").unwrap()
            - c.default_value("sick_personal_hours").unwrap()
            - c.default_value("training_hours").unwrap();
        assert!((available - 1784.0).abs() < 1e-9);
        let productive = available * (1.0 - c.default_value("non_service_percent").unwrap() / 100.0);
        assert!((productive - 1427.2).abs() < 1e-9);
    }
}
