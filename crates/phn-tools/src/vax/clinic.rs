//! VAX-08 `plan_vaccination_clinic`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;
use crate::vax::content::{self, Basis, CLINIC_INFO, DoseBand};

/// Arguments for `plan_vaccination_clinic`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ClinicArgs {
    /// Expected number of people vaccinated.
    pub expected_turnout: u32,
    /// Hours the clinic is open for vaccination.
    pub clinic_hours: f64,
    /// Average doses per person (default 1; e.g. 2 for flu plus COVID-19).
    pub doses_per_person: Option<f64>,
    /// Vaccines offered, e.g. ["influenza", "COVID-19"] (one reconciliation row each).
    pub vaccines: Vec<String>,
    /// "children", "adolescents", "adults", or "mixed" (default). Affects the
    /// anaphylaxis kit (pediatric autoinjector doses).
    pub population: Option<String>,
    /// Doses in each vial for multidose vials (omit for single-dose syringes).
    pub doses_per_vial: Option<u32>,
    /// Override: doses one vaccinator gives per hour.
    pub doses_per_vaccinator_per_hour: Option<f64>,
    /// Override: check-ins per registration staff member per hour.
    pub registrations_per_hour: Option<f64>,
    /// Override: screenings per screener per hour.
    pub screenings_per_hour: Option<f64>,
    /// Override: observation minutes after vaccination.
    pub observation_minutes: Option<f64>,
    /// Override: extra vaccine and supplies for waste, in percent.
    pub waste_factor_pct: Option<f64>,
    /// Look up the epinephrine dose for a person of this weight (kg).
    pub epinephrine_weight_kg: Option<f64>,
    /// Write the dose reconciliation sheet (XLSX) to /data/output (default true).
    pub reconciliation_sheet: Option<bool>,
}

/// One planning assumption and where it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AssumptionUsed {
    pub id: String,
    pub label: String,
    pub value: f64,
    pub unit: String,
    /// "tool_default" (the tool's own figure), "source" (cited), "computed",
    /// or "your_override".
    pub basis: String,
    pub source: Option<String>,
    pub note: String,
}

/// Staffing by station.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StaffLine {
    pub station: String,
    pub people: u32,
    pub how: String,
}

/// A supply line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SupplyLine {
    pub item: String,
    pub quantity: u32,
    pub basis: Basis,
}

/// Epinephrine lookup for one weight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EpinephrineLookup {
    pub weight_kg: f64,
    /// 0.01 mg/kg, capped at 0.5 mg (1 mg/mL solution: the same number of mL).
    pub calculated_mg: f64,
    /// The age/weight band from the DPH standing order and Immunize.org chart.
    pub band: Option<DoseBand>,
    pub note: String,
}

/// Result of `plan_vaccination_clinic`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClinicPlan {
    pub arrivals_per_hour: f64,
    pub doses_total: u32,
    pub doses_per_hour: f64,
    pub staffing: Vec<StaffLine>,
    pub total_staff: u32,
    pub observation_seats: u32,
    pub vaccine_doses_to_bring: u32,
    pub vials_to_bring: Option<u32>,
    pub supplies: Vec<SupplyLine>,
    pub assumptions: Vec<AssumptionUsed>,
    pub stations: Vec<String>,
    pub layout: Vec<String>,
    pub anaphylaxis_kit: Vec<String>,
    pub anaphylaxis_protocol: Vec<String>,
    pub epinephrine_table: Vec<DoseBand>,
    pub diphenhydramine_table: Vec<DoseBand>,
    pub epinephrine_lookup: Option<EpinephrineLookup>,
    pub reconciliation_file: Option<String>,
}

fn assumption(id: &str, over: Option<f64>) -> Result<AssumptionUsed> {
    let a = CLINIC_INFO
        .assumption(id)
        .ok_or_else(|| PhnError::Internal(format!("assumption {id} missing")))?;
    Ok(match over {
        Some(v) => AssumptionUsed {
            id: a.id.clone(),
            label: a.label.clone(),
            value: v,
            unit: a.unit.clone(),
            basis: "your_override".into(),
            source: None,
            note: format!("You set this (the tool default is {} {}).", a.value, a.unit),
        },
        None => AssumptionUsed {
            id: a.id.clone(),
            label: a.label.clone(),
            value: a.value,
            unit: a.unit.clone(),
            basis: match a.basis {
                Basis::ToolDefault => "tool_default",
                Basis::Source => "source",
                Basis::Computed => "computed",
            }
            .into(),
            source: a.source.clone(),
            note: a.note.clone(),
        },
    })
}

fn positive(name: &str, v: Option<f64>) -> Result<Option<f64>> {
    match v {
        Some(x) if !(x.is_finite() && x > 0.0) => Err(PhnError::InvalidInput(format!("{name} must be a positive number"))),
        other => Ok(other),
    }
}

/// Plan a clinic from expected turnout.
pub fn plan_vaccination_clinic(ctx: &Context, args: ClinicArgs) -> Result<Envelope<ClinicPlan>> {
    if args.expected_turnout == 0 {
        return Err(PhnError::InvalidInput("expected_turnout must be at least 1".into()));
    }
    if !(args.clinic_hours.is_finite() && args.clinic_hours > 0.0 && args.clinic_hours <= 24.0) {
        return Err(PhnError::InvalidInput("clinic_hours must be more than 0 and at most 24".into()));
    }
    let per_person = positive("doses_per_person", args.doses_per_person)?.unwrap_or(1.0);
    let vacc_rate = assumption("doses_per_vaccinator_per_hour", positive("doses_per_vaccinator_per_hour", args.doses_per_vaccinator_per_hour)?)?;
    let reg_rate = assumption("registrations_per_hour", positive("registrations_per_hour", args.registrations_per_hour)?)?;
    let scr_rate = assumption("screenings_per_hour", positive("screenings_per_hour", args.screenings_per_hour)?)?;
    let obs_min = assumption("observation_minutes", positive("observation_minutes", args.observation_minutes)?)?;
    let waste = match args.waste_factor_pct {
        Some(w) if !(0.0..=100.0).contains(&w) => {
            return Err(PhnError::InvalidInput("waste_factor_pct must be between 0 and 100".into()));
        }
        other => assumption("waste_factor_pct", other)?,
    };
    let per_monitor = assumption("observation_seats_per_monitor", None)?;
    let predraw = assumption("max_predrawn_per_vaccinator", None)?;
    let epi_min = assumption("epinephrine_doses_on_site", None)?;
    let lead = assumption("clinical_lead", None)?;

    let hours = args.clinic_hours;
    let arrivals = args.expected_turnout as f64 / hours;
    let doses_total = (args.expected_turnout as f64 * per_person).ceil() as u32;
    let doses_per_hour = doses_total as f64 / hours;
    let ceil = |x: f64| x.ceil().max(1.0) as u32;
    let vaccinators = ceil(doses_per_hour / vacc_rate.value);
    let registrars = ceil(arrivals / reg_rate.value);
    let screeners = ceil(arrivals / scr_rate.value);
    let seats = ceil(arrivals * obs_min.value / 60.0 * 1.25);
    let monitors = ceil(seats as f64 / per_monitor.value);
    let staffing = vec![
        StaffLine { station: "Check-in and registration".into(), people: registrars, how: format!("{arrivals:.1} arrivals/hour ÷ {} per person", reg_rate.value) },
        StaffLine { station: "Screening".into(), people: screeners, how: format!("{arrivals:.1} arrivals/hour ÷ {} per screener", scr_rate.value) },
        StaffLine { station: "Vaccination".into(), people: vaccinators, how: format!("{doses_per_hour:.1} doses/hour ÷ {} per vaccinator", vacc_rate.value) },
        StaffLine { station: "Observation".into(), people: monitors, how: format!("{seats} seats ÷ {} per observer", per_monitor.value) },
        StaffLine { station: "Clinical lead (anaphylaxis response, not vaccinating)".into(), people: lead.value as u32, how: "tool default".into() },
    ];
    let total_staff = staffing.iter().map(|s| s.people).sum();
    let doses_bring = (doses_total as f64 * (1.0 + waste.value / 100.0)).ceil() as u32;
    let vials = match args.doses_per_vial {
        Some(0) => return Err(PhnError::InvalidInput("doses_per_vial must be at least 1".into())),
        Some(n) => Some(doses_bring.div_ceil(n)),
        None => None,
    };
    let mut supplies: Vec<SupplyLine> = CLINIC_INFO
        .supplies_per_dose
        .iter()
        .map(|s| SupplyLine { item: s.item.clone(), quantity: (s.per_dose * doses_bring as f64).ceil() as u32, basis: s.basis })
        .collect();
    supplies.push(SupplyLine {
        item: format!("Sharps containers (about {} doses each)", CLINIC_INFO.sharps_container_doses),
        quantity: doses_bring.div_ceil(CLINIC_INFO.sharps_container_doses).max(vaccinators),
        basis: CLINIC_INFO.sharps_container_basis,
    });
    supplies.push(SupplyLine { item: "Epinephrine doses (at least, per site)".into(), quantity: epi_min.value as u32, basis: Basis::Source });

    let pop = args.population.as_deref().unwrap_or("mixed").trim().to_ascii_lowercase();
    if !["children", "adolescents", "adults", "mixed", "child", "adult", "teens"].contains(&pop.as_str()) {
        return Err(PhnError::InvalidInput("population must be children, adolescents, adults, or mixed".into()));
    }
    let mut kit: Vec<String> = CLINIC_INFO.anaphylaxis_kit.first_line.clone();
    if pop.starts_with("adult") {
        kit.push("Adults: epinephrine 0.3 mg autoinjectors (DPH Standing Order 2026-04).".into());
    } else {
        kit.push("Children: stock autoinjector strengths for the ages served (0.1 mg for 7.5-14 kg, 0.15 mg, 0.3 mg) or 1 mg/mL vials with 1 mL syringes for weight-based dosing (see table).".into());
    }
    kit.extend(CLINIC_INFO.anaphylaxis_kit.optional.iter().map(|s| format!("Optional: {s}")));
    kit.extend(CLINIC_INFO.anaphylaxis_kit.supplies.iter().cloned());

    let epinephrine_lookup = match args.epinephrine_weight_kg {
        Some(w) if !(w.is_finite() && w > 0.0 && w < 300.0) => {
            return Err(PhnError::InvalidInput("epinephrine_weight_kg must be a weight in kilograms".into()));
        }
        Some(w) => {
            let mg = (w * 0.01).min(0.5);
            let band = CLINIC_INFO
                .epinephrine
                .bands
                .iter()
                .find(|b| w >= b.min_kg - 0.5 && b.max_kg.is_none_or(|m| w <= m + 0.5))
                .cloned();
            Some(EpinephrineLookup {
                weight_kg: w,
                calculated_mg: (mg * 100.0).round() / 100.0,
                band,
                note: format!(
                    "0.01 mg/kg of 1 mg/mL (1:1000) epinephrine IM, maximum 0.5 mg per dose (minimum measurable dose {} mL); may repeat 2 more times at 5-15 minute intervals while waiting for EMS. Confirm against your signed standing order.",
                    CLINIC_INFO.epinephrine.minimum_solution_dose_ml
                ),
            })
        }
        None => None,
    };

    let mut assumptions = vec![vacc_rate, reg_rate, scr_rate, obs_min, waste, per_monitor, predraw, epi_min, lead];
    assumptions.push(AssumptionUsed {
        id: "observation_seat_margin".into(),
        label: "Margin added to observation seats".into(),
        value: 25.0,
        unit: "percent".into(),
        basis: "tool_default".into(),
        source: None,
        note: "Seats = arrivals per hour × observation minutes ÷ 60 × 1.25.".into(),
    });

    // Reconciliation sheet.
    let mut files_written = Vec::new();
    let mut reconciliation_file = None;
    if args.reconciliation_sheet.unwrap_or(true) {
        let names: Vec<String> = if args.vaccines.is_empty() { vec![String::new(); 3] } else { args.vaccines.clone() };
        let rows: Vec<Vec<String>> = names
            .iter()
            .map(|v| {
                let mut r = vec![String::new(); CLINIC_INFO.reconciliation_columns.len()];
                r[0] = v.clone();
                r
            })
            .collect();
        let plan_rows: Vec<Vec<String>> = staffing
            .iter()
            .map(|s| vec![s.station.clone(), s.people.to_string(), s.how.clone()])
            .chain([
                vec!["Vaccine doses to bring".into(), doses_bring.to_string(), format!("{doses_total} doses + {}% waste", assumptions[4].value)],
                vec!["Observation seats".into(), seats.to_string(), "see assumptions".into()],
            ])
            .collect();
        let doc = Document {
            title: "Vaccination clinic dose reconciliation".into(),
            subtitle: None,
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks: vec![
                Block::Paragraph(vec![Inline::plain("Count every vial and syringe at the start and end of the clinic. Report doses to the MIIS. Explain any difference before closing the clinic.")]),
                Block::Table(TableBlock { caption: Some("Dose reconciliation".into()), headers: CLINIC_INFO.reconciliation_columns.clone(), rows }),
                Block::Table(TableBlock { caption: Some("Clinic plan".into()), headers: vec!["Item".into(), "Number".into(), "How".into()], rows: plan_rows }),
            ],
        };
        let bytes = phn_docs::xlsx::to_xlsx(&doc)?;
        let sandbox = ctx.sandbox()?;
        let path = sandbox.new_output_file("clinic_dose_reconciliation.xlsx")?;
        std::fs::write(&path, bytes)?;
        let disp = sandbox.display_path(&path);
        files_written.push(disp.clone());
        reconciliation_file = Some(disp);
    }

    // Summary.
    let mut summary = format!(
        "**Vaccination clinic plan** — {} people over {hours} hour(s) ({arrivals:.1} arrivals/hour), {doses_total} doses ({doses_per_hour:.1}/hour)\n\n",
        args.expected_turnout
    );
    let rows: Vec<Vec<String>> = staffing.iter().map(|s| vec![s.station.clone(), s.people.to_string(), s.how.clone()]).collect();
    summary.push_str(&md::table(&["Station", "Staff", "Calculation"], &rows));
    summary.push_str(&format!(
        "\n**Total staff on the floor: {total_staff}** (add relief for breaks on clinics over 4 hours). Observation seats: {seats}.\n\nBring **{doses_bring} doses**{} and:\n",
        vials.map(|v| format!(" ({v} vials of {})", args.doses_per_vial.unwrap_or(1))).unwrap_or_default()
    ));
    for s in &supplies {
        summary.push_str(&format!("- {} × {}{}\n", s.quantity, s.item, if s.basis == Basis::ToolDefault { " (tool default)" } else { "" }));
    }
    summary.push_str("\n**Assumptions** (override any of them)\n\n");
    let rows: Vec<Vec<String>> = assumptions
        .iter()
        .map(|a| vec![a.label.clone(), format!("{} {}", a.value, a.unit), a.basis.replace('_', " "), a.source.clone().unwrap_or_default()])
        .collect();
    summary.push_str(&md::table(&["Assumption", "Value", "Basis", "Source"], &rows));
    summary.push_str("\n**Anaphylaxis kit** (DPH standing orders 2026-03/2026-04; Immunize.org)\n");
    for k in &kit {
        summary.push_str(&format!("- {k}\n"));
    }
    summary.push_str("\n**Epinephrine by age and weight** (1 mg/mL IM; 0.01 mg/kg up to 0.5 mg; may repeat 2 more times every 5-15 minutes)\n\n");
    let rows: Vec<Vec<String>> = CLINIC_INFO
        .epinephrine
        .bands
        .iter()
        .map(|b| vec![b.age_group.clone(), format!("{} lb", b.weight_lb), format!("{}{} kg", b.min_kg, b.max_kg.map(|m| format!("-{m}")).unwrap_or_else(|| "+".into())), b.solution_dose.clone().unwrap_or_default(), b.autoinjector.clone().unwrap_or_default()])
        .collect();
    summary.push_str(&md::table(&["Age", "Weight (lb)", "Weight (kg)", "1 mg/mL dose", "Autoinjector"], &rows));
    summary.push_str(&format!("{}\n", CLINIC_INFO.epinephrine.weight_preferred_note));
    for n in &CLINIC_INFO.epinephrine.intranasal {
        summary.push_str(&format!("- {n}\n"));
    }
    if let Some(e) = &epinephrine_lookup {
        summary.push_str(&format!(
            "\n**For {} kg:** {} mg ({} mL of 1 mg/mL){}. {}\n",
            e.weight_kg,
            e.calculated_mg,
            e.calculated_mg,
            e.band.as_ref().map(|b| format!("; chart band {} — autoinjector {}", b.age_group, b.autoinjector.clone().unwrap_or_default())).unwrap_or_default(),
            e.note
        ));
    }
    summary.push_str(&format!("\nDiphenhydramine (optional, for itching and hives only): {}\n", CLINIC_INFO.diphenhydramine.rule));
    summary.push_str("\n**Emergency protocol**\n");
    for s in &CLINIC_INFO.anaphylaxis_protocol {
        summary.push_str(&format!("{}. {}\n", s.order, s.text));
    }
    summary.push_str("\n**Layout**\n");
    for l in &CLINIC_INFO.layout_guidance {
        summary.push_str(&format!("- {l}\n"));
    }
    if let Some(f) = &reconciliation_file {
        summary.push_str(&format!("\nDose reconciliation sheet written to `{f}`.\n"));
    }

    let mut sources = vec![content::content_source(content::CLINIC)?];
    if let Ok(m) = content::meta(content::CLINIC) {
        for r in &m.related_sources {
            sources.push(Source {
                title: r.title.clone(),
                publisher: r.publisher.clone(),
                url: r.url.clone(),
                effective_date: None,
                retrieved: Some(m.prepared.clone()),
                mode: SourceMode::Snapshot,
            });
        }
    }
    let mut caveats = vec![
        "Throughput, staffing ratios, waste, and supplies marked 'tool default' are planning assumptions with no published source; replace them with your agency's experience.".to_string(),
        "The DPH anaphylaxis standing orders (2026-03 and 2026-04) authorize qualified local board of health personnel only, when the vaccine was given under a DPH standing order; other settings need their own medical director's order. Dosing tables must be checked against the signed order.".to_string(),
        format!("Predrawing: {}", CLINIC_INFO.assumption("max_predrawn_per_vaccinator").map(|a| a.note.clone()).unwrap_or_default()),
    ];
    caveats.extend(content::review_caveats(&[content::CLINIC]));
    Ok(Envelope::new(
        summary,
        ClinicPlan {
            arrivals_per_hour: (arrivals * 10.0).round() / 10.0,
            doses_total,
            doses_per_hour: (doses_per_hour * 10.0).round() / 10.0,
            staffing,
            total_staff,
            observation_seats: seats,
            vaccine_doses_to_bring: doses_bring,
            vials_to_bring: vials,
            supplies,
            assumptions,
            stations: CLINIC_INFO.stations.iter().map(|s| format!("{}: {}", s.label, s.text)).collect(),
            layout: CLINIC_INFO.layout_guidance.clone(),
            anaphylaxis_kit: kit,
            anaphylaxis_protocol: CLINIC_INFO.anaphylaxis_protocol.iter().map(|s| s.text.clone()).collect(),
            epinephrine_table: CLINIC_INFO.epinephrine.bands.clone(),
            diphenhydramine_table: CLINIC_INFO.diphenhydramine.bands.clone(),
            epinephrine_lookup,
            reconciliation_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        files_written,
        ..Privacy::none()
    })
    .decision_support())
}
