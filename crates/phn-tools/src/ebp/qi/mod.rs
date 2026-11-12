//! EBP-05 `plan_quality_improvement`: guided quality improvement planning with the Model for
//! Improvement. It turns what the nurse tells it into an aim statement with checks, a family of
//! outcome, process, and balancing measures, a driver diagram, a fishbone diagram, a process map,
//! PDSA cycle plans, and a SQUIRE 2.0 reporting checklist; analyzes the nurse's measure data
//! with run chart rules (IHI/Perla and Anhøj) or control charts; and writes a DOCX plan, a
//! measurement tracker with live formulas, and the diagrams and chart to /data/output.
//!
//! Content is `content/ebp/quality_improvement.yaml` (pending review). Run chart rules are in
//! [`rules`] (golden tests against an independent computation); control limits and the Anhøj
//! rules come from `phn_epi::spc`.

pub mod analysis;
pub mod content;
pub mod diagrams;
pub mod plan_doc;
pub mod rules;
pub mod tracker;
pub mod types;

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::Document;
use phn_local::Sandbox;

use self::content::{QI_CONTENT, QiContent};
use self::diagrams::{CauseCategory, Diagram, MapStep, PrimaryDriver, SecondaryDriver, StepKind};
use self::plan_doc::{PlanImage, PlanImages};
pub use self::types::*;
use crate::context::{Context, error_message};
use crate::epr::content as econtent;
use crate::epr::content::CitedOut;
use crate::epr::roster;
use crate::md;

/// Fields that hold file paths, column names, or dates, which are not scanned for contact details.
const SKIP_KEYS: &[&str] = &[
    "path",
    "sheet",
    "date_column",
    "flag_column",
    "value_column",
    "period_column",
    "numerator_column",
    "denominator_column",
    "when",
    "by",
];

const MAX_TEXT: usize = 600;

fn text(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn check_len(s: &str, field: &str) -> Result<()> {
    if s.chars().count() > MAX_TEXT {
        return Err(PhnError::InvalidInput(format!(
            "{field} is longer than {MAX_TEXT} characters; shorten it"
        )));
    }
    Ok(())
}

fn num(x: f64) -> String {
    let s = format!("{x:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn with_unit(x: f64, unit: Option<&str>) -> String {
    match unit {
        Some("%") => format!("{}%", num(x)),
        Some(u) if !u.is_empty() => format!("{} {u}", num(x)),
        _ => num(x),
    }
}

fn check(id: &str, label: &str, status: CheckStatus, note: impl Into<String>) -> Check {
    Check {
        id: id.into(),
        label: label.into(),
        status,
        note: note.into(),
    }
}

// ---------------------------------------------------------------- aim

fn by_text(by: &str) -> Result<String> {
    use chrono::Datelike;
    if let Some(d) = phn_local::phi::parse_date(by) {
        if !(1900..=2100).contains(&d.year()) {
            return Err(PhnError::InvalidInput(
                "the aim's 'by' date must be between 1900 and 2100".into(),
            ));
        }
        return Ok(d.format("%B %-d, %Y").to_string());
    }
    let b = by.as_bytes();
    let looks_iso = b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    if looks_iso {
        return Err(PhnError::InvalidInput(
            "the aim's 'by' date is not a valid date between 1900 and 2100".into(),
        ));
    }
    Ok(by.to_string())
}

fn build_aim(c: &QiContent, a: &AimInput) -> Result<AimOut> {
    for (v, f) in [
        (&a.what, "aim.what"),
        (&a.population, "aim.population"),
        (&a.unit, "aim.unit"),
        (&a.by, "aim.by"),
    ] {
        if let Some(s) = v {
            check_len(s, f)?;
        }
    }
    for (v, f) in [(a.baseline, "aim.baseline"), (a.goal, "aim.goal")] {
        if v.is_some_and(|x| !x.is_finite()) {
            return Err(PhnError::InvalidInput(format!("{f} must be a number")));
        }
    }
    let unit = text(&a.unit);
    let what = text(&a.what);
    let pop = text(&a.population);
    let by = match text(&a.by) {
        Some(b) => Some(by_text(b)?),
        None => None,
    };
    let verb = a.direction.map(Direction::verb);
    let statement = what.map(|w| {
        c.aim
            .template
            .replace("{direction}", verb.unwrap_or("[increase or decrease]"))
            .replace("{what}", w)
            .replace("{population}", pop.unwrap_or("[population and setting]"))
            .replace(
                "{baseline}",
                &a.baseline
                    .map(|x| with_unit(x, unit))
                    .unwrap_or_else(|| "[baseline]".into()),
            )
            .replace(
                "{goal}",
                &a.goal
                    .map(|x| with_unit(x, unit))
                    .unwrap_or_else(|| "[goal]".into()),
            )
            .replace("{by}", by.as_deref().unwrap_or("[date]"))
    });
    let mut checks = Vec::new();
    for at in &c.aim.attributes {
        let (status, note) = match at.id.as_str() {
            "population" => match pop {
                Some(_) => (CheckStatus::Met, "A population is named.".to_string()),
                None => (
                    CheckStatus::Missing,
                    "Say for whom: the group and setting the improvement is for.".into(),
                ),
            },
            "numeric" => match (a.baseline, a.goal) {
                (Some(b), Some(g)) => (
                    CheckStatus::Met,
                    format!(
                        "Gap: {} to {} (a change of {}).",
                        with_unit(b, unit),
                        with_unit(g, unit),
                        num((g - b).abs())
                    ),
                ),
                _ => (
                    CheckStatus::Missing,
                    "Give the baseline and the goal as numbers.".into(),
                ),
            },
            "time_bound" => match &by {
                Some(_) => (CheckStatus::Met, "A time frame is given.".into()),
                None => (CheckStatus::Missing, "Say by when.".into()),
            },
            "direction" => match (a.direction, a.baseline, a.goal) {
                (None, _, _) => (
                    CheckStatus::Missing,
                    "Say whether you want the measure to increase or decrease.".into(),
                ),
                (Some(_), Some(b), Some(g)) if (g - b).abs() < f64::EPSILON => {
                    (CheckStatus::Attention, "The goal equals the baseline.".into())
                }
                (Some(Direction::Increase), Some(b), Some(g)) if g < b => (
                    CheckStatus::Attention,
                    "You chose increase, but the goal is below the baseline.".into(),
                ),
                (Some(Direction::Decrease), Some(b), Some(g)) if g > b => (
                    CheckStatus::Attention,
                    "You chose decrease, but the goal is above the baseline.".into(),
                ),
                _ => (CheckStatus::Met, "Direction and goal agree.".into()),
            },
            _ => (CheckStatus::Judgment, at.note.clone()),
        };
        checks.push(check(&at.id, &at.label, status, note));
    }
    Ok(AimOut {
        statement,
        template: c.aim.template.clone(),
        checks,
        examples: econtent::cite_all(&c.aim.examples, &c.sources),
    })
}

// ---------------------------------------------------------------- measures

fn build_measures(c: &QiContent, ms: &[MeasureInput]) -> Result<MeasuresOut> {
    if ms.len() > 20 {
        return Err(PhnError::InvalidInput("give at most 20 measures".into()));
    }
    let cite = |src: &str, sec: &str| {
        c.sources
            .get(src)
            .map(|s| format!("{}, {sec}", s.title))
            .unwrap_or_else(|| sec.to_string())
    };
    let types: Vec<MeasureTypeOut> = c
        .measure_types
        .iter()
        .map(|t| MeasureTypeOut {
            id: t.id.clone(),
            label: t.label.clone(),
            plain: t.plain.clone(),
            prompts: t.prompts.clone(),
            example: t.example.clone(),
            citation: cite(&t.source, &t.section),
            confidence: t.confidence,
        })
        .collect();
    let mut rows = Vec::new();
    for m in ms {
        if m.name.trim().is_empty() {
            return Err(PhnError::InvalidInput("every measure needs a name".into()));
        }
        for (v, f) in [
            (Some(&m.name), "measure name"),
            (m.numerator.as_ref(), "numerator"),
            (m.denominator.as_ref(), "denominator"),
            (m.data_source.as_ref(), "data_source"),
            (m.frequency.as_ref(), "frequency"),
            (m.sampling.as_ref(), "sampling"),
            (m.owner.as_ref(), "owner"),
            (m.goal.as_ref(), "goal"),
        ] {
            if let Some(s) = v {
                check_len(s, f)?;
            }
        }
        let mut missing = Vec::new();
        for (v, f) in [
            (&m.numerator, "numerator"),
            (&m.data_source, "data source"),
            (&m.frequency, "frequency"),
            (&m.owner, "owner"),
        ] {
            if text(v).is_none() {
                missing.push(f.to_string());
            }
        }
        rows.push(MeasureRowOut {
            name: m.name.trim().to_string(),
            kind: m.kind.id().to_string(),
            numerator: text(&m.numerator).map(str::to_string),
            denominator: text(&m.denominator).map(str::to_string),
            data_source: text(&m.data_source).map(str::to_string),
            frequency: text(&m.frequency).map(str::to_string),
            sampling: text(&m.sampling).map(str::to_string),
            owner: text(&m.owner).map(str::to_string),
            goal: text(&m.goal).map(str::to_string),
            missing,
        });
    }
    let mut checks = Vec::new();
    for k in [
        MeasureKind::Outcome,
        MeasureKind::Process,
        MeasureKind::Balancing,
    ] {
        let have = ms.iter().filter(|m| m.kind == k).count();
        let def = c.measure_types.iter().find(|t| t.id == k.id());
        let label = format!("At least one {} measure", k.id());
        let (status, note) = if have > 0 {
            (CheckStatus::Met, format!("{have} listed."))
        } else {
            (
                CheckStatus::Missing,
                def.map(|d| d.prompts.join(" ")).unwrap_or_default(),
            )
        };
        checks.push(check(k.id(), &label, status, note));
    }
    let incomplete = rows.iter().filter(|r| !r.missing.is_empty()).count();
    checks.push(check(
        "operational_definitions",
        "Each measure has an operational definition",
        if rows.is_empty() {
            CheckStatus::Missing
        } else if incomplete == 0 {
            CheckStatus::Met
        } else {
            CheckStatus::Attention
        },
        if rows.is_empty() {
            "List your measures with a numerator, data source, frequency, and owner.".to_string()
        } else if incomplete == 0 {
            "Numerator, data source, frequency, and owner are filled in.".into()
        } else {
            format!("{incomplete} measure(s) still lack part of the definition.")
        },
    ));
    Ok(MeasuresOut {
        types,
        measures: rows,
        checks,
        definition_fields: c
            .operational_definition_fields
            .iter()
            .map(|f| format!("{}: {}", f.label, f.prompt))
            .collect(),
        note: econtent::cite_all(
            std::slice::from_ref(&c.operational_definition_note),
            &c.sources,
        )
        .remove(0),
    })
}

// ---------------------------------------------------------------- diagrams

fn diagram_out(
    d: &Diagram,
    scaffold: bool,
    files: (Option<String>, Option<String>),
    guide: Vec<CitedOut>,
) -> DiagramOut {
    DiagramOut {
        is_scaffold: scaffold,
        mermaid: d.mermaid.clone(),
        outline: d.outline.clone(),
        alt_text: d.alt_text.clone(),
        svg_file: files.0,
        png_file: files.1,
        guidance: guide,
    }
}

fn build_drivers(ds: &[DriverInput]) -> Result<Vec<PrimaryDriver>> {
    if ds.len() > 10 {
        return Err(PhnError::InvalidInput(
            "give at most 10 primary drivers".into(),
        ));
    }
    let mut out = Vec::new();
    for d in ds {
        if d.primary.trim().is_empty() {
            return Err(PhnError::InvalidInput(
                "every driver needs a primary driver name".into(),
            ));
        }
        check_len(&d.primary, "primary driver")?;
        if d.secondary.len() > 10 {
            return Err(PhnError::InvalidInput(
                "give at most 10 secondary drivers for each primary driver".into(),
            ));
        }
        let mut sec = Vec::new();
        for s in &d.secondary {
            if s.name.trim().is_empty() {
                return Err(PhnError::InvalidInput(
                    "every secondary driver needs a name".into(),
                ));
            }
            check_len(&s.name, "secondary driver")?;
            if s.change_ideas.len() > 10 {
                return Err(PhnError::InvalidInput(
                    "give at most 10 change ideas for each secondary driver".into(),
                ));
            }
            for c in &s.change_ideas {
                check_len(c, "change idea")?;
            }
            sec.push(SecondaryDriver {
                name: s.name.trim().to_string(),
                change_ideas: s
                    .change_ideas
                    .iter()
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty())
                    .collect(),
            });
        }
        out.push(PrimaryDriver {
            name: d.primary.trim().to_string(),
            secondary: sec,
        });
    }
    Ok(out)
}

fn build_categories(c: &QiContent, f: Option<&FishboneInput>) -> Result<Vec<CauseCategory>> {
    let given: Vec<&CategoryInput> = f.map(|f| f.categories.iter().collect()).unwrap_or_default();
    if given.len() > 10 {
        return Err(PhnError::InvalidInput(
            "give at most 10 fishbone categories".into(),
        ));
    }
    if given.is_empty() {
        return Ok(c
            .fishbone
            .default_categories
            .iter()
            .map(|d| CauseCategory {
                name: d.name.clone(),
                causes: Vec::new(),
            })
            .collect());
    }
    let mut out = Vec::new();
    for g in given {
        if g.name.trim().is_empty() {
            return Err(PhnError::InvalidInput(
                "every fishbone category needs a name".into(),
            ));
        }
        check_len(&g.name, "fishbone category")?;
        if g.causes.len() > 15 {
            return Err(PhnError::InvalidInput(
                "give at most 15 causes for each fishbone category".into(),
            ));
        }
        for x in &g.causes {
            check_len(x, "cause")?;
        }
        out.push(CauseCategory {
            name: g.name.trim().to_string(),
            causes: g
                .causes
                .iter()
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect(),
        });
    }
    Ok(out)
}

fn build_steps(ps: &[ProcessStepInput]) -> Result<Vec<MapStep>> {
    if ps.len() > 40 {
        return Err(PhnError::InvalidInput(
            "give at most 40 process steps".into(),
        ));
    }
    let mut out = Vec::new();
    for p in ps {
        if p.name.trim().is_empty() {
            return Err(PhnError::InvalidInput("every step needs a name".into()));
        }
        check_len(&p.name, "process step")?;
        if let Some(o) = &p.owner {
            check_len(o, "step owner")?;
        }
        out.push(MapStep {
            name: p.name.trim().to_string(),
            kind: match p.kind.unwrap_or(StepKindInput::Step) {
                StepKindInput::Start => StepKind::Start,
                StepKindInput::Step => StepKind::Step,
                StepKindInput::Decision => StepKind::Decision,
                StepKindInput::End => StepKind::End,
            },
            owner: text(&p.owner).map(str::to_string),
            pain_point: p.pain_point.unwrap_or(false),
            no_goes_to: None,
        });
    }
    for (i, p) in ps.iter().enumerate() {
        let Some(t) = text(&p.no_goes_to) else {
            continue;
        };
        if out[i].kind != StepKind::Decision {
            return Err(PhnError::InvalidInput(format!(
                "step {}: no_goes_to is only for decision steps",
                i + 1
            )));
        }
        let target = t
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=out.len()).contains(n))
            .map(|n| n - 1)
            .or_else(|| out.iter().position(|s| s.name.eq_ignore_ascii_case(t)))
            .ok_or_else(|| {
                PhnError::InvalidInput(format!(
                    "step {}: no_goes_to '{t}' is not a step number (1 to {}) or a step name",
                    i + 1,
                    out.len()
                ))
            })?;
        out[i].no_goes_to = Some(target);
    }
    Ok(out)
}

// ---------------------------------------------------------------- PDSA

fn build_pdsa(c: &QiContent, cycles: &[PdsaInput], has_frequency: bool) -> Result<PdsaOut> {
    if cycles.len() > 20 {
        return Err(PhnError::InvalidInput("give at most 20 PDSA cycles".into()));
    }
    let mut rows = Vec::new();
    for (i, p) in cycles.iter().enumerate() {
        if p.change.trim().is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "PDSA cycle {} needs the change to test",
                i + 1
            )));
        }
        for (v, f) in [
            (Some(&p.change), "change"),
            (p.objective.as_ref(), "objective"),
            (p.prediction.as_ref(), "prediction"),
            (p.who.as_ref(), "who"),
            (p.when.as_ref(), "when"),
            (p.where_.as_ref(), "where"),
            (p.scale.as_ref(), "scale"),
            (p.data_to_collect.as_ref(), "data_to_collect"),
            (p.builds_on.as_ref(), "builds_on"),
        ] {
            if let Some(s) = v {
                check_len(s, f)?;
            }
        }
        let mut missing = Vec::new();
        for (v, f) in [
            (&p.objective, "objective"),
            (&p.prediction, "prediction"),
            (&p.who, "who"),
            (&p.when, "when"),
            (&p.scale, "scale"),
            (&p.data_to_collect, "data to collect"),
        ] {
            if text(v).is_none() {
                missing.push(f.to_string());
            }
        }
        if i > 0 && text(&p.builds_on).is_none() {
            missing.push("builds on".into());
        }
        rows.push(CycleOut {
            number: i + 1,
            change: p.change.trim().to_string(),
            objective: text(&p.objective).map(str::to_string),
            prediction: text(&p.prediction).map(str::to_string),
            who: text(&p.who).map(str::to_string),
            when: text(&p.when).map(str::to_string),
            where_: text(&p.where_).map(str::to_string),
            scale: text(&p.scale).map(str::to_string),
            data_to_collect: text(&p.data_to_collect).map(str::to_string),
            builds_on: text(&p.builds_on).map(str::to_string),
            missing,
        });
    }
    let none_yet = "No cycles planned yet.".to_string();
    let all = |f: &dyn Fn(&CycleOut) -> bool| !rows.is_empty() && rows.iter().all(f);
    let mut checks = Vec::new();
    for q in &c.pdsa.quality_checks {
        let (status, note) = match q.id.as_str() {
            "prediction" => {
                if rows.is_empty() {
                    (CheckStatus::Missing, none_yet.clone())
                } else if all(&|r| r.prediction.is_some()) {
                    (CheckStatus::Met, "Every cycle states a prediction.".into())
                } else {
                    (
                        CheckStatus::Missing,
                        "Write a prediction for each cycle before the test starts.".into(),
                    )
                }
            }
            "small_scale" => {
                if rows.is_empty() {
                    (CheckStatus::Missing, none_yet.clone())
                } else if all(&|r| r.scale.is_some()) {
                    (CheckStatus::Met, "Every cycle states its scale.".into())
                } else {
                    (
                        CheckStatus::Missing,
                        "Say how small each test is (people, visits, days).".into(),
                    )
                }
            }
            "data_over_time" => {
                if rows.is_empty() {
                    (CheckStatus::Missing, none_yet.clone())
                } else if all(&|r| r.data_to_collect.is_some()) && has_frequency {
                    (
                        CheckStatus::Met,
                        "Each cycle names its data and a measure has a frequency.".into(),
                    )
                } else {
                    (
                        CheckStatus::Missing,
                        "Name the data for each cycle and give a measure a frequency so results are plotted over time.".into(),
                    )
                }
            }
            "linked_cycles" => match rows.len() {
                0 => (CheckStatus::Missing, none_yet.clone()),
                1 => (
                    CheckStatus::Judgment,
                    "Plan the next cycle from what the first one teaches.".into(),
                ),
                _ => {
                    if rows.iter().skip(1).all(|r| r.builds_on.is_some()) {
                        (
                            CheckStatus::Met,
                            "Later cycles say what they build on.".into(),
                        )
                    } else {
                        (
                            CheckStatus::Attention,
                            "Say how each later cycle builds on the one before.".into(),
                        )
                    }
                }
            },
            "one_change" => {
                if rows.is_empty() {
                    (CheckStatus::Missing, none_yet.clone())
                } else if rows.iter().any(|r| {
                    let l = r.change.to_lowercase();
                    l.contains(" and ") || l.contains(';')
                }) {
                    (
                        CheckStatus::Attention,
                        "A change description joins two things (it has 'and' or ';'). Test them in separate cycles if you can.".into(),
                    )
                } else {
                    (CheckStatus::Met, "Each cycle names one change.".into())
                }
            }
            "scale_up" => (
                CheckStatus::Judgment,
                "Increase the scale (more people, sites, or days) as confidence grows.".into(),
            ),
            _ => (
                CheckStatus::Judgment,
                "Fill in Do, Study, and Act for each cycle in the PDSA sheet of the tracker."
                    .into(),
            ),
        };
        checks.push(check(&q.id, &q.label, status, note));
    }
    Ok(PdsaOut {
        description: econtent::cite_all(std::slice::from_ref(&c.pdsa.description), &c.sources)
            .remove(0),
        stages: c
            .pdsa
            .stages
            .iter()
            .map(|s| StageOut {
                id: s.id.clone(),
                label: s.label.clone(),
                questions: s.questions.clone(),
            })
            .collect(),
        cycles: rows,
        checks,
    })
}

// ---------------------------------------------------------------- SQUIRE

fn plan_section_name(id: &str) -> &'static str {
    match id {
        "aim" => "Aim statement",
        "measures" => "Measures",
        "drivers" => "Driver diagram",
        "fishbone" => "Fishbone diagram",
        "process_map" => "Process map",
        "pdsa" => "PDSA cycles",
        "analysis" => "Run chart or control chart",
        _ => "Plan",
    }
}

fn build_squire(c: &QiContent, status: &[SquireStatusInput]) -> Result<SquireOut> {
    if status.len() > 18 {
        return Err(PhnError::InvalidInput(
            "squire_status has more than 18 entries".into(),
        ));
    }
    for s in status {
        if !(1..=18).contains(&s.item) {
            return Err(PhnError::InvalidInput(format!(
                "squire_status item {} is not between 1 and 18",
                s.item
            )));
        }
        if let Some(w) = &s.where_addressed {
            check_len(w, "where_addressed")?;
        }
    }
    let items: Vec<SquireItemOut> = c
        .squire
        .items
        .iter()
        .map(|i| {
            let st = status.iter().rev().find(|s| s.item == i.number);
            SquireItemOut {
                number: i.number,
                name: i.name.clone(),
                section: i.section.clone(),
                purpose: i.purpose.clone(),
                ask: i.ask.clone(),
                status: st.map(|s| s.status).unwrap_or_default(),
                where_addressed: st.and_then(|s| text(&s.where_addressed).map(str::to_string)),
                plan_section: plan_section_name(&i.tool_section).into(),
            }
        })
        .collect();
    let count = |s: SquireState| items.iter().filter(|i| i.status == s).count();
    Ok(SquireOut {
        url: c.squire.url.clone(),
        license_note: c
            .squire
            .license_note
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        cite_as: c.squire.cite_as.clone(),
        note_to_authors: econtent::cite_all(
            std::slice::from_ref(&c.squire.note_to_authors),
            &c.sources,
        )
        .remove(0),
        done: count(SquireState::Done),
        in_progress: count(SquireState::InProgress),
        todo: count(SquireState::Todo),
        not_applicable: count(SquireState::NotApplicable),
        items,
    })
}

// ---------------------------------------------------------------- files

fn output_stem(title: Option<&str>) -> String {
    let base = title
        .map(|t| t.chars().take(40).collect::<String>())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| "project".into());
    phn_local::sandbox::sanitize_file_name(&format!("qi_{base}"))
}

fn write_bytes(sb: &Sandbox, name: &str, bytes: &[u8]) -> Result<String> {
    let p = sb.new_output_file(name)?;
    std::fs::write(&p, bytes)?;
    Ok(sb.display_path(&p))
}

/// Write a diagram as SVG and PNG; returns the two file names and the image for the DOCX plan.
#[allow(clippy::type_complexity)]
fn write_diagram(
    sb: &Sandbox,
    stem: &str,
    name: &str,
    d: &Diagram,
    files: &mut Vec<String>,
    caveats: &mut Vec<String>,
) -> ((Option<String>, Option<String>), Option<PlanImage>) {
    let mut out = (None, None);
    match write_bytes(sb, &format!("{stem}_{name}.svg"), d.svg.as_bytes()) {
        Ok(p) => {
            files.push(p.clone());
            out.0 = Some(p);
        }
        Err(e) => caveats.push(format!(
            "The {name} SVG was not written: {}",
            error_message(&e)
        )),
    }
    let png = match phn_docs::svg::svg_to_png(&d.svg, 2.0) {
        Ok(p) => p,
        Err(e) => {
            caveats.push(format!(
                "The {name} image could not be drawn: {}",
                error_message(&e)
            ));
            return (out, None);
        }
    };
    match write_bytes(sb, &format!("{stem}_{name}.png"), &png) {
        Ok(p) => {
            files.push(p.clone());
            out.1 = Some(p);
        }
        Err(e) => caveats.push(format!(
            "The {name} PNG was not written: {}",
            error_message(&e)
        )),
    }
    let img = PlanImage {
        png,
        path: out.1.clone().or(out.0.clone()).unwrap_or_default(),
        alt: d.alt_text.clone(),
    };
    (out, Some(img))
}

// ---------------------------------------------------------------- the tool

/// Guided quality improvement planning with the Model for Improvement.
pub fn plan_quality_improvement(ctx: &Context, args: QiPlanArgs) -> Result<Envelope<QiPlanResult>> {
    let c: QiContent = econtent::load(QI_CONTENT)?;
    for (v, f) in [
        (&args.project_title, "project_title"),
        (&args.problem, "problem"),
        (&args.measure_name, "measure_name"),
    ] {
        if let Some(s) = v {
            check_len(s, f)?;
        }
    }
    let wants = |s: Section| args.sections.is_empty() || args.sections.contains(&s);
    let data_given = analysis::has_data(&args);
    if args.sections.contains(&Section::Analysis) && !data_given {
        return Err(PhnError::InvalidInput(
            "the analysis section needs your measure data: give values (with labels) or a path to a local file".into(),
        ));
    }
    let want_analysis = data_given && wants(Section::Analysis);

    // ---- build every part
    let aim_out = if wants(Section::Aim) {
        Some(build_aim(&c, &args.aim.clone().unwrap_or_default())?)
    } else {
        None
    };
    let measures_out = wants(Section::Measures)
        .then(|| build_measures(&c, &args.measures))
        .transpose()?;
    let driver_defs = build_drivers(&args.drivers)?;
    let categories = build_categories(&c, args.fishbone.as_ref())?;
    let steps = build_steps(&args.process_steps)?;
    let has_frequency = args.measures.iter().any(|m| text(&m.frequency).is_some());
    let pdsa_out = wants(Section::Pdsa)
        .then(|| build_pdsa(&c, &args.pdsa_cycles, has_frequency))
        .transpose()?;
    let squire_out = wants(Section::Squire)
        .then(|| build_squire(&c, &args.squire_status))
        .transpose()?;

    let aim_statement = aim_out.as_ref().and_then(|a| a.statement.clone());
    let aim_for_diagram = aim_statement
        .clone()
        .or_else(|| text(&args.problem).map(str::to_string))
        .unwrap_or_default();
    let direction = args.aim.as_ref().and_then(|a| a.direction);

    let driver_d =
        wants(Section::Drivers).then(|| diagrams::driver_diagram(&aim_for_diagram, &driver_defs));
    let fish_problem = args
        .fishbone
        .as_ref()
        .and_then(|f| text(&f.problem))
        .map(str::to_string)
        .or_else(|| text(&args.problem).map(str::to_string))
        .unwrap_or_default();
    let fish_d = wants(Section::Fishbone).then(|| diagrams::fishbone(&fish_problem, &categories));
    let map_d = wants(Section::ProcessMap).then(|| diagrams::process_map(&steps));

    // ---- analysis
    let analysis_run = if want_analysis {
        let prep = analysis::prepare(ctx, &args)?;
        Some(analysis::analyze(ctx, &args, prep, direction, &c)?)
    } else {
        None
    };

    // ---- files
    let mut files: Vec<String> = Vec::new();
    let mut caveats: Vec<String> = Vec::new();
    let write_diagrams = args.write_diagrams.unwrap_or(true);
    let write_plan = args.write_plan.unwrap_or(true);
    let write_tracker = args.write_tracker.unwrap_or(true);
    let stem = output_stem(args.project_title.as_deref());
    let sandbox = if write_diagrams || write_plan || write_tracker {
        match ctx.sandbox() {
            Ok(s) => Some(s),
            Err(e) => {
                caveats.push(format!(
                    "No files were written: the output folder is not available ({}).",
                    error_message(&e)
                ));
                None
            }
        }
    } else {
        None
    };
    let mut images = PlanImages::default();
    let mut file_names: [(Option<String>, Option<String>); 3] = Default::default();
    if let (Some(sb), true) = (&sandbox, write_diagrams) {
        for (i, (d, name)) in [
            (&driver_d, "driver_diagram"),
            (&fish_d, "fishbone"),
            (&map_d, "process_map"),
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(d) = d {
                let (names, img) = write_diagram(sb, &stem, name, d, &mut files, &mut caveats);
                file_names[i] = names;
                match i {
                    0 => images.driver = img,
                    1 => images.fishbone = img,
                    _ => images.process_map = img,
                }
            }
        }
    }
    let [driver_files, fish_files, map_files] = file_names;

    let driver_out = driver_d.as_ref().map(|d| {
        diagram_out(
            d,
            driver_defs.is_empty(),
            driver_files,
            econtent::cite_all(
                &[
                    c.driver_diagram.description.clone(),
                    c.driver_diagram.reading_tip.clone(),
                    c.driver_diagram.change_package_note.clone(),
                ],
                &c.sources,
            ),
        )
    });
    let fish_out = fish_d.as_ref().map(|d| {
        let mut g = vec![
            c.fishbone.description.clone(),
            c.fishbone.category_note.clone(),
        ];
        g.extend(c.fishbone.steps.clone());
        g.push(c.fishbone.caution.clone());
        diagram_out(
            d,
            args.fishbone
                .as_ref()
                .map(|f| f.categories.iter().all(|k| k.causes.is_empty()))
                .unwrap_or(true),
            fish_files,
            econtent::cite_all(&g, &c.sources),
        )
    });
    let map_out = map_d.as_ref().map(|d| {
        let mut g = econtent::cite_all(std::slice::from_ref(&c.process_map.description), &c.sources);
        g.extend(c.process_map.tips.iter().map(|t| CitedOut {
            text: t.clone(),
            citation: "Drafting guidance by this tool (not quoted from a source)".into(),
            confidence: econtent::Confidence::Low,
        }));
        diagram_out(d, steps.is_empty(), map_files, g)
    });
    if let Some(a) = &analysis_run {
        files.extend(a.files.iter().cloned());
        caveats.extend(a.caveats.iter().cloned());
        if let (Some(p), Some(bytes), false) = (&a.png_path, &a.png, a.any_hidden) {
            images.run_chart = Some(PlanImage {
                png: bytes.clone(),
                path: p.clone(),
                alt: a.out.text_alternative.clone(),
            });
        }
    }

    // ---- result
    let questions: Vec<QuestionOut> = c
        .model_for_improvement
        .questions
        .iter()
        .map(|q| QuestionOut {
            number: q.number,
            question: q.question.clone(),
            how_the_tool_helps: q.how_the_tool_helps.clone(),
            citation: c
                .sources
                .get(&q.source)
                .map(|s| format!("{}, {}", s.title, q.section))
                .unwrap_or_else(|| q.section.clone()),
        })
        .collect();
    let mut result = QiPlanResult {
        project_title: text(&args.project_title).map(str::to_string),
        questions,
        aim: aim_out,
        measures: measures_out,
        driver_diagram: driver_out,
        fishbone: fish_out,
        process_map: map_out,
        pdsa: pdsa_out,
        analysis: analysis_run.as_ref().map(|a| a.out.clone()),
        squire: squire_out,
        files: Vec::new(),
        next_steps: Vec::new(),
    };

    // ---- documents
    let review_note = econtent::review_caveats(&[QI_CONTENT])
        .into_iter()
        .next()
        .unwrap_or_else(|| "Content reviewed.".into());
    let title = text(&args.project_title)
        .unwrap_or("Quality improvement project")
        .to_string();
    let source_keys = |for_doc: bool| -> Vec<&'static str> {
        let mut keys = vec!["core-components", "pdsa-review"];
        if result.fishbone.is_some() {
            keys.push("fishbone-primer");
        }
        if result.process_map.is_some() || result.measures.is_some() {
            keys.push("process-improvement-methods");
        }
        if !for_doc && (result.measures.is_some() || result.driver_diagram.is_some()) {
            keys.push("learning-health-system");
        }
        if want_analysis {
            keys.extend(["run-charts-2014", "run-charts-2015", "run-chart-2011"]);
        }
        if result.squire.is_some() {
            keys.push("squire2");
        }
        keys
    };
    let source_lines: Vec<String> = source_keys(true)
        .iter()
        .filter_map(|k| c.sources.get(*k))
        .map(|s| format!("{} ({})", s.title, s.url))
        .collect();
    if let Some(sb) = &sandbox {
        if write_plan {
            let chart_note = analysis_run.as_ref().and_then(|a| {
                a.any_hidden.then(|| {
                    "The chart is saved in the charts folder of your output folder. It shows small counts that were hidden in the chat response, so check it against your agency's rules before sharing.".to_string()
                })
            });
            let blocks = plan_doc::blocks(
                &result,
                &images,
                &review_note,
                &source_lines,
                chart_note.as_deref(),
            );
            let doc = Document {
                title: format!("{title}: quality improvement plan"),
                subtitle: Some("Draft for review".into()),
                language: "en-US".into(),
                author: None,
                letterhead: ctx.settings().letterhead,
                logo: None,
                blocks,
            };
            match phn_docs::docx::to_docx(&doc)
                .and_then(|bytes| write_bytes(sb, &format!("{stem}_plan.docx"), &bytes))
            {
                Ok(p) => files.push(p),
                Err(e) => caveats.push(format!("The DOCX plan was not written: {}", error_message(&e))),
            }
        }
        if write_tracker {
            let (labels, nums, dens, base): (Vec<String>, Vec<f64>, Option<Vec<f64>>, usize) =
                match &analysis_run {
                    Some(a) => (
                        a.labels.clone(),
                        a.numerators.clone(),
                        a.denominators.clone(),
                        a.baseline_points.unwrap_or(a.labels.len()).max(12.min(a.labels.len())),
                    ),
                    None => (
                        Vec::new(),
                        Vec::new(),
                        None,
                        args.baseline_points.unwrap_or(12),
                    ),
                };
            let measures_rows = result
                .measures
                .as_ref()
                .map(|m| m.measures.clone())
                .unwrap_or_default();
            let cycles = result
                .pdsa
                .as_ref()
                .map(|p| p.cycles.clone())
                .unwrap_or_default();
            let squire_items = result
                .squire
                .as_ref()
                .map(|s| s.items.clone())
                .unwrap_or_default();
            let input = tracker::TrackerInput {
                title: &title,
                aim: aim_statement.as_deref(),
                measures: &measures_rows,
                labels: &labels,
                numerators: &nums,
                denominators: dens.as_deref(),
                baseline_points: base,
                measure_name: text(&args.measure_name),
                pdsa: &cycles,
                squire: &squire_items,
                review_note: &review_note,
            };
            match tracker::build(&input)
                .and_then(|bytes| write_bytes(sb, &format!("{stem}_tracker.xlsx"), &bytes))
            {
                Ok(p) => files.push(p),
                Err(e) => caveats.push(format!(
                    "The tracker workbook was not written: {}",
                    error_message(&e)
                )),
            }
        }
    }
    result.files = files.clone();

    // ---- next steps
    let mut next = Vec::new();
    if let Some(a) = &result.aim
        && a.checks.iter().any(|k| k.status == CheckStatus::Missing)
    {
        next.push("Finish the aim: fill in what the aim checks list as missing.".to_string());
    }
    if let Some(m) = &result.measures
        && m.checks.iter().any(|k| k.status == CheckStatus::Missing)
    {
        next.push("Add the missing measure types (outcome, process, balancing) with operational definitions.".into());
    }
    if result.driver_diagram.as_ref().is_some_and(|d| d.is_scaffold) {
        next.push("Build the driver diagram with your team, then re-run with the drivers filled in.".into());
    }
    if result.pdsa.as_ref().is_some_and(|p| p.cycles.is_empty()) {
        next.push("Plan the first small PDSA test, with a prediction.".into());
    }
    if analysis_run.is_none() {
        next.push("Collect baseline data (12 to 20 periods if you can) and re-run with values, or a path to your file, to see the run chart.".into());
    }
    next.push("Before you publish or present, decide with your agency whether this is quality improvement or research that needs review.".into());
    result.next_steps = next;

    // ---- privacy
    let texts = roster::argument_strings(&args, SKIP_KEYS);
    let found = roster::contacts_in_arguments(texts.iter().map(String::as_str));
    let mut privacy = Privacy {
        suppression_threshold: analysis_run
            .as_ref()
            .and_then(|a| a.local_source.as_ref())
            .map(|_| ctx.suppression_threshold()),
        ..Privacy::none()
    };
    privacy.files_written = files.clone();
    if let Some(a) = &analysis_run {
        privacy.suppression_applied = a.suppression_applied;
        privacy.identifiers_detected = a.identifier_categories.clone();
        if a.local_source.is_some() {
            privacy.notes.push(
                "Rows of your file were grouped into periods and only period results are returned; values based on fewer records than the suppression threshold are hidden here, but are in the chart and tracker written to your output folder.".into(),
            );
            if a.out.values_withheld {
                privacy.notes.push(
                    "The median or center line, control limits, and phase averages are withheld from this response because, with the other values, they would reveal the hidden numbers.".into(),
                );
            }
        }
    }
    if !found.is_empty() {
        for f in &found {
            if !privacy.identifiers_detected.contains(f) {
                privacy.identifiers_detected.push(f.clone());
            }
        }
        privacy.notes.push(
            "Contact details or other identifiers were found in the text of your request. A QI plan needs roles, not people's names, numbers, or addresses: remove them before you share the plan.".into(),
        );
    }
    if !files.is_empty() {
        privacy.notes.push(
            "The plan, tracker, diagrams, and chart were written to the output folder only. The tracker is for aggregate numbers: do not enter names or record numbers.".into(),
        );
    }

    // ---- summary, caveats, sources
    let summary = summary_markdown(&c, &result, &title, &files);
    caveats.extend(econtent::review_caveats(&[QI_CONTENT]));
    caveats.push("The Model for Improvement and PDSA wording was drafted from open-access articles; IHI's own pages (https://www.ihi.org/) and The Improvement Guide could not be read from the build environment, so check the wording against IHI's current tools. Items marked low confidence in the content were not verified in a fetched source.".into());
    if result.squire.is_some() {
        caveats.push("SQUIRE 2.0 is a copyrighted guideline. This tool lists item numbers and names with its own paraphrases and links to the guideline; read the full text and cite it (see the checklist's license note).".into());
    }
    if want_analysis {
        caveats.push(format!(
            "Run chart rules: the Anhøj limits match the published table (PLoS One 2014). The shift (6) and trend (5) lengths follow Perla's rules as described in the Anhøj papers, because the Perla paper's full text could not be fetched. The too-few and too-many runs limits come from the exact distribution of runs (2.5% in each tail), and the astronomical-point screen (modified z-score above {}) is this tool's own stand-in for a judgment call; neither was compared with Perla's table or wording.",
            rules::ASTRONOMICAL_Z
        ));
    }
    caveats.push("A QI plan is decision support for your team. Whether the work is quality improvement or human subjects research is a determination for your agency or IRB, not this tool.".into());

    let mut source_list: Vec<Source> = econtent::sources_for(source_keys(false), &c.sources);
    if want_analysis {
        source_list.push(Source::computed(
            "Run chart rules and control chart limits (IHI/Perla and Anhøj rules; XmR, p, u, c limits)",
            "PHN MCP server calculation (Anhøj limits checked against the published table; control limits against R qcc and an independent computation)",
        ));
    }
    if let Some(src) = analysis_run.as_ref().and_then(|a| a.local_source.clone()) {
        source_list.push(src);
    }

    Ok(Envelope::new(summary, result)
        .sources(source_list)
        .caveats(caveats)
        .privacy(privacy)
        .decision_support())
}

fn summary_markdown(c: &QiContent, r: &QiPlanResult, title: &str, files: &[String]) -> String {
    let mut s = format!("**{title}: quality improvement plan** (Model for Improvement)\n\n");
    s.push_str("The three questions: ");
    s.push_str(
        &r.questions
            .iter()
            .map(|q| format!("{}. {}", q.number, q.question))
            .collect::<Vec<_>>()
            .join(" "),
    );
    s.push_str("\n\n");
    if let Some(a) = &r.aim {
        s.push_str("**Aim**\n\n");
        match &a.statement {
            Some(st) => s.push_str(&format!("> {st}\n\n")),
            None => s.push_str(&format!(
                "No aim yet. Template: _{}_ (say what changes, for whom, from what to what, by when).\n\n",
                a.template
            )),
        }
        let todo: Vec<String> = a
            .checks
            .iter()
            .filter(|k| k.status != CheckStatus::Met)
            .map(|k| format!("- {}: {}", k.label, k.note))
            .collect();
        if !todo.is_empty() {
            s.push_str("Aim checks to settle:\n");
            s.push_str(&todo.join("\n"));
            s.push_str("\n\n");
        }
    }
    if let Some(m) = &r.measures {
        s.push_str("**Measures** (outcome, process, balancing)\n\n");
        if m.measures.is_empty() {
            for t in &m.types {
                s.push_str(&format!("- {}: {} Example: {}\n", t.label, t.plain, t.example));
            }
        } else {
            let rows: Vec<Vec<String>> = m
                .measures
                .iter()
                .map(|x| {
                    vec![
                        x.name.clone(),
                        x.kind.clone(),
                        x.numerator.clone().unwrap_or_default(),
                        x.denominator.clone().unwrap_or_default(),
                        x.frequency.clone().unwrap_or_default(),
                        x.missing.join(", "),
                    ]
                })
                .collect();
            s.push_str(&md::table(
                &[
                    "Measure",
                    "Type",
                    "Numerator",
                    "Denominator",
                    "Frequency",
                    "Still missing",
                ],
                &rows,
            ));
        }
        for k in m.checks.iter().filter(|k| k.status == CheckStatus::Missing) {
            s.push_str(&format!("- Missing: {}. {}\n", k.label, k.note));
        }
        s.push('\n');
    }
    for (heading, d) in [
        ("Driver diagram", &r.driver_diagram),
        ("Fishbone diagram", &r.fishbone),
        ("Process map", &r.process_map),
    ] {
        if let Some(d) = d {
            s.push_str(&format!(
                "**{heading}**{}\n\n",
                if d.is_scaffold {
                    " (blank scaffold to fill in with your team)"
                } else {
                    ""
                }
            ));
            s.push_str(&format!("{}\n\n```mermaid\n{}```\n\n", d.alt_text, d.mermaid));
        }
    }
    if let Some(p) = &r.pdsa {
        s.push_str("**PDSA cycles**\n\n");
        if p.cycles.is_empty() {
            for st in &p.stages {
                s.push_str(&format!("- {}: {}\n", st.label, st.questions.join(" ")));
            }
        } else {
            let rows: Vec<Vec<String>> = p
                .cycles
                .iter()
                .map(|x| {
                    vec![
                        x.number.to_string(),
                        x.change.clone(),
                        x.prediction.clone().unwrap_or_default(),
                        x.scale.clone().unwrap_or_default(),
                        x.missing.join(", "),
                    ]
                })
                .collect();
            s.push_str(&md::table(
                &["Cycle", "Change tested", "Prediction", "Scale", "Still missing"],
                &rows,
            ));
        }
        for k in p
            .checks
            .iter()
            .filter(|k| matches!(k.status, CheckStatus::Missing | CheckStatus::Attention))
        {
            s.push_str(&format!("- {}: {}\n", k.label, k.note));
        }
        s.push('\n');
    }
    if let Some(a) = &r.analysis {
        s.push_str(&format!(
            "**Analysis** of {} ({} periods, {} chart)\n\n",
            a.data_from,
            a.periods,
            match a.chart_type {
                phn_epi::spc::ChartType::Run => "run",
                phn_epi::spc::ChartType::Xmr => "XmR",
                phn_epi::spc::ChartType::P => "p",
                phn_epi::spc::ChartType::U => "u",
                phn_epi::spc::ChartType::C => "c",
            }
        ));
        for ph in &a.phases {
            let mut line = format!("- {} to {}:", ph.first_period, ph.last_period);
            if let Some(m) = ph.center {
                line.push_str(&format!(" center {}", md::num(m, 2)));
            }
            if let (Some(l), Some(lim)) = (ph.longest_run, ph.longest_run_limit) {
                line.push_str(&format!("; longest run {l} (Anhøj limit {lim})"));
            } else if let Some(l) = ph.longest_run {
                line.push_str(&format!("; longest run {l}"));
            }
            if let (Some(x), Some(lim)) = (ph.crossings, ph.crossings_limit) {
                line.push_str(&format!("; crossings {x} (at least {lim} expected)"));
            }
            if let Some(t) = ph.longest_trend {
                line.push_str(&format!("; longest trend {t}"));
            }
            if let Some(x) = ph.runs {
                line.push_str(&format!("; runs {x}"));
            }
            s.push_str(&line);
            s.push('\n');
        }
        s.push('\n');
        if a.findings.is_empty() {
            s.push_str("**No signals of non-random variation.**\n\n");
        } else {
            s.push_str("**Signals**\n");
            for f in &a.findings {
                s.push_str(&format!(
                    "- [{}] {}: {}. {}\n",
                    f.rule_set, f.rule, f.periods, f.meaning
                ));
            }
            s.push('\n');
        }
        for n in &a.notes {
            s.push_str(&format!("- {n}\n"));
        }
        s.push_str(&format!(
            "\nChart text alternative: {}\n\n",
            a.text_alternative
        ));
        if let Some(m) = &a.mermaid_chart {
            s.push_str(&format!("```mermaid\n{m}```\n\n"));
        }
    }
    if let Some(q) = &r.squire {
        s.push_str(&format!(
            "**SQUIRE 2.0 checklist** (18 items): {} done, {} in progress, {} to do, {} not applicable. Items: {}.\n\n",
            q.done,
            q.in_progress,
            q.todo,
            q.not_applicable,
            q.items
                .iter()
                .map(|i| format!("{} {}", i.number, i.name))
                .collect::<Vec<_>>()
                .join("; ")
        ));
        s.push_str(&format!(
            "SQUIRE is copyrighted; this checklist paraphrases and links ({}). Cite it when you use it.\n\n",
            c.squire.url
        ));
    }
    if !files.is_empty() {
        s.push_str("**Written to the output folder**\n");
        for f in files {
            s.push_str(&format!("- `{f}`\n"));
        }
        s.push('\n');
    }
    if !r.next_steps.is_empty() {
        s.push_str("**Next steps**\n");
        for n in &r.next_steps {
            s.push_str(&format!("- {n}\n"));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLINIC: &str = include_str!("../../../../../fixtures/synthetic/data/qi_clinic_measure.csv");

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(dir.path().join("data/qi_clinic_measure.csv"), CLINIC).unwrap();
        let mut c = phn_core::Config::for_root(dir.path());
        c.offline = true;
        (dir, Context::new(c).unwrap())
    }

    fn full_args() -> QiPlanArgs {
        QiPlanArgs {
            project_title: Some("Vaccine reminders".into()),
            problem: Some("Too many two-year-olds are behind on vaccines".into()),
            aim: Some(AimInput {
                direction: Some(Direction::Increase),
                what: Some("the share of two-year-olds up to date on recommended vaccines".into()),
                population: Some("children 24-35 months at the Elm Street clinic".into()),
                baseline: Some(68.0),
                goal: Some(80.0),
                unit: Some("%".into()),
                by: Some("2027-06-30".into()),
            }),
            measures: vec![
                MeasureInput {
                    name: "Percent up to date".into(),
                    kind: MeasureKind::Outcome,
                    numerator: Some("Children 24-35 months with all recommended doses".into()),
                    denominator: Some("Children 24-35 months seen".into()),
                    data_source: Some("Immunization registry report".into()),
                    frequency: Some("monthly".into()),
                    sampling: None,
                    owner: Some("Clinic nurse".into()),
                    goal: Some("80%".into()),
                },
                MeasureInput {
                    name: "Percent with a reminder sent".into(),
                    kind: MeasureKind::Process,
                    numerator: None,
                    denominator: None,
                    data_source: None,
                    frequency: None,
                    sampling: None,
                    owner: None,
                    goal: None,
                },
            ],
            drivers: vec![DriverInput {
                primary: "Reliable reminders".into(),
                secondary: vec![SecondaryInput {
                    name: "Accurate contact list".into(),
                    change_ideas: vec!["Verify phone numbers at every visit".into()],
                }],
            }],
            process_steps: vec![
                ProcessStepInput {
                    name: "Child arrives".into(),
                    kind: Some(StepKindInput::Start),
                    ..Default::default()
                },
                ProcessStepInput {
                    name: "Record reviewed?".into(),
                    kind: Some(StepKindInput::Decision),
                    no_goes_to: Some("1".into()),
                    pain_point: Some(true),
                    ..Default::default()
                },
                ProcessStepInput {
                    name: "Vaccines given".into(),
                    kind: Some(StepKindInput::End),
                    ..Default::default()
                },
            ],
            pdsa_cycles: vec![
                PdsaInput {
                    change: "Call families of 5 children due this month".into(),
                    prediction: Some("At least 3 of 5 will book a visit".into()),
                    scale: Some("One nurse, 5 families, one week".into()),
                    objective: Some("Learn whether calls work".into()),
                    who: Some("Clinic nurse".into()),
                    when: Some("First week of March".into()),
                    data_to_collect: Some("Visits booked".into()),
                    ..Default::default()
                },
                PdsaInput {
                    change: "Call 20 families".into(),
                    ..Default::default()
                },
            ],
            squire_status: vec![SquireStatusInput {
                item: 6,
                status: SquireState::Done,
                where_addressed: Some("Introduction".into()),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_blank_request_gives_scaffolds_and_files() {
        let (dir, ctx) = ctx();
        let r = plan_quality_improvement(&ctx, QiPlanArgs::default()).unwrap();
        let d = &r.data;
        assert_eq!(d.questions.len(), 3);
        let a = d.aim.as_ref().unwrap();
        assert!(a.statement.is_none());
        assert!(a.checks.iter().any(|c| c.id == "population" && c.status == CheckStatus::Missing));
        assert!(d.driver_diagram.as_ref().unwrap().is_scaffold);
        assert!(d.fishbone.as_ref().unwrap().is_scaffold);
        assert!(d.process_map.as_ref().unwrap().is_scaffold);
        assert!(d.analysis.is_none());
        assert_eq!(d.squire.as_ref().unwrap().items.len(), 18);
        assert_eq!(d.squire.as_ref().unwrap().todo, 18);
        // Three diagrams as svg + png, the plan, and the tracker.
        assert_eq!(d.files.len(), 8, "{:?}", d.files);
        for f in &d.files {
            assert!(dir.path().join("data").join(f).exists(), "{f}");
        }
        assert!(r.caveats.iter().any(|c| c.contains("has not yet been reviewed")));
        assert!(r.notice.is_some());
        assert!(!r.privacy.row_level_data);
    }

    #[test]
    fn a_full_plan_builds_the_aim_checks_and_pdsa_findings() {
        let (_dir, ctx) = ctx();
        let r = plan_quality_improvement(&ctx, full_args()).unwrap();
        let d = &r.data;
        let a = d.aim.as_ref().unwrap();
        assert_eq!(
            a.statement.as_deref(),
            Some("We will increase the share of two-year-olds up to date on recommended vaccines for children 24-35 months at the Elm Street clinic from 68% to 80% by June 30, 2027.")
        );
        assert!(a.checks.iter().filter(|c| c.id != "feasible").all(|c| c.status == CheckStatus::Met));
        let m = d.measures.as_ref().unwrap();
        assert!(m.checks.iter().any(|c| c.id == "balancing" && c.status == CheckStatus::Missing));
        assert_eq!(m.measures[1].missing, vec!["numerator", "data source", "frequency", "owner"]);
        let p = d.pdsa.as_ref().unwrap();
        assert_eq!(p.cycles.len(), 2);
        assert!(p.cycles[1].missing.contains(&"builds on".to_string()));
        let status = |id: &str| p.checks.iter().find(|c| c.id == id).unwrap().status;
        assert_eq!(status("prediction"), CheckStatus::Missing);
        assert_eq!(status("linked_cycles"), CheckStatus::Attention);
        let map = d.process_map.as_ref().unwrap();
        assert!(!map.is_scaffold && map.mermaid.contains("N2 -->|No| N1"));
        let s = d.squire.as_ref().unwrap();
        assert_eq!((s.done, s.todo), (1, 17));
        assert_eq!(s.items[5].where_addressed.as_deref(), Some("Introduction"));
        assert!(s.license_note.contains("copyrighted"));
        assert!(r.summary.contains("> We will increase"));
        assert!(r.summary.contains("```mermaid"));
    }

    #[test]
    fn bad_input_is_a_tool_error() {
        let (_d, ctx) = ctx();
        let mut a = full_args();
        a.pdsa_cycles[0].change = "  ".into();
        assert!(plan_quality_improvement(&ctx, a).unwrap_err().to_string().contains("needs the change"));
        let mut a = full_args();
        a.process_steps[1].no_goes_to = Some("Nowhere".into());
        assert!(plan_quality_improvement(&ctx, a).unwrap_err().to_string().contains("not a step number"));
        let mut a = full_args();
        a.squire_status[0].item = 19;
        assert!(plan_quality_improvement(&ctx, a).unwrap_err().to_string().contains("between 1 and 18"));
        let mut a = full_args();
        a.aim.as_mut().unwrap().by = Some("1850-01-01".into());
        assert!(plan_quality_improvement(&ctx, a).unwrap_err().to_string().contains("1900 and 2100"));
        let a = QiPlanArgs {
            sections: vec![Section::Analysis],
            ..Default::default()
        };
        assert!(plan_quality_improvement(&ctx, a).unwrap_err().to_string().contains("needs your measure data"));
        let a = QiPlanArgs {
            values: Some(vec![1.0, 2.0]),
            path: Some("qi_clinic_measure.csv".into()),
            ..Default::default()
        };
        assert!(plan_quality_improvement(&ctx, a).is_err());
        let a = QiPlanArgs {
            values: Some(vec![1.0, 2.0, 3.0, 4.0]),
            chart_type: Some(phn_epi::spc::ChartType::P),
            ..Default::default()
        };
        assert!(plan_quality_improvement(&ctx, a).unwrap_err().to_string().contains("denominator"));
    }

    fn step_series() -> QiPlanArgs {
        let mut v = vec![4., 5., 4., 6., 5., 4., 5., 6., 4., 5., 4., 5.];
        v.extend([9., 10., 9., 11., 10., 9., 10., 11.]);
        QiPlanArgs {
            values: Some(v),
            labels: Some((1..=20).map(|i| format!("M{i}")).collect()),
            baseline_points: Some(12),
            measure_name: Some("Reminder calls per week".into()),
            aim: Some(AimInput {
                direction: Some(Direction::Increase),
                ..Default::default()
            }),
            sections: vec![Section::Analysis],
            ..Default::default()
        }
    }

    #[test]
    fn inline_run_chart_finds_the_shift_with_both_rule_sets() {
        let (dir, ctx) = ctx();
        let r = plan_quality_improvement(&ctx, step_series()).unwrap();
        let a = r.data.analysis.as_ref().unwrap();
        assert_eq!(a.periods, 20);
        assert!(a.findings.iter().any(|f| f.rule_set == "ihi" && f.rule == "shift" && f.periods == "M13 to M20"));
        assert!(a.findings.iter().any(|f| f.rule_set == "anhoej" && f.rule == "unusually long run"));
        assert!(a.findings.iter().any(|f| f.meaning.contains("direction your aim wants")));
        assert!(a.rows[15].signals.contains(&"shift".to_string()));
        assert_eq!(a.phases.len(), 1);
        assert_eq!(a.phases[0].center, Some(5.0));
        assert_eq!(a.phases[0].longest_run_limit, Some(7));
        assert!(a.mermaid_chart.as_ref().unwrap().starts_with("xychart-beta"));
        assert!(a.text_alternative.contains("Run chart: Reminder calls per week"));
        assert!(!a.values_withheld);
        // chart png + svg, plan, tracker (diagrams not requested by `sections`).
        assert_eq!(r.data.files.len(), 4, "{:?}", r.data.files);
        for f in &r.data.files {
            assert!(dir.path().join("data").join(f).exists());
        }
        assert!(r.data.driver_diagram.is_none() && r.data.squire.is_none());
    }

    #[test]
    fn rule_set_and_phases_are_honored() {
        let (_d, ctx) = ctx();
        let mut a = step_series();
        a.rule_set = Some(RuleSet::Anhoej);
        a.write_plan = Some(false);
        a.write_tracker = Some(false);
        let r = plan_quality_improvement(&ctx, a).unwrap();
        let an = r.data.analysis.unwrap();
        assert!(an.findings.iter().all(|f| f.rule_set == "anhoej"));
        assert!(an.phases[0].longest_trend.is_none());
        let mut a = step_series();
        a.phase_starts = Some(vec!["M13".into()]);
        a.baseline_points = Some(6);
        let r = plan_quality_improvement(&ctx, a).unwrap();
        let an = r.data.analysis.unwrap();
        assert_eq!(an.phases.len(), 2);
        assert!(an.findings.iter().all(|f| f.rule != "shift"));
        assert!(an.notes.iter().any(|n| n.contains("new median starts")));
        let mut a = step_series();
        a.phase_starts = Some(vec!["nope".into()]);
        assert!(plan_quality_improvement(&ctx, a).is_err());
    }

    #[test]
    fn control_charts_use_the_spc_engine() {
        let (_d, ctx) = ctx();
        let a = QiPlanArgs {
            values: Some(vec![33., 41., 32., 30., 30., 35., 36., 37., 28., 39., 43., 39.]),
            denominators: Some(vec![50., 60., 47., 43., 46., 54., 54., 57., 43., 47., 53., 49.]),
            chart_type: Some(phn_epi::spc::ChartType::P),
            sections: vec![Section::Analysis],
            write_plan: Some(false),
            write_tracker: Some(false),
            ..Default::default()
        };
        let r = plan_quality_improvement(&ctx, a).unwrap();
        let an = r.data.analysis.unwrap();
        assert_eq!(an.chart_type, phn_epi::spc::ChartType::P);
        assert!(an.rule_set.is_none());
        assert!(an.rows[0].ucl.unwrap() > an.rows[0].lcl.unwrap());
        // Percent display.
        assert!((an.rows[0].value.unwrap() - 66.0).abs() < 1e-9);
    }

    #[test]
    fn a_period_file_is_analyzed_and_small_counts_are_hidden() {
        let (dir, ctx) = ctx();
        let a = QiPlanArgs {
            path: Some("qi_clinic_measure.csv".into()),
            period_column: Some("Month".into()),
            numerator_column: Some("Children Up To Date".into()),
            denominator_column: Some("Children Seen".into()),
            baseline_points: Some(9),
            phase_starts: Some(vec!["2025-10".into()]),
            measure_name: Some("% up to date".into()),
            sections: vec![Section::Analysis],
            ..Default::default()
        };
        let r = plan_quality_improvement(&ctx, a).unwrap();
        let an = r.data.analysis.as_ref().unwrap();
        assert_eq!(an.periods, 20);
        // 2026-04 has 3 of 6: both below the threshold of 5, so it is hidden, and the median and
        // limits are withheld.
        let april = an.rows.iter().find(|x| x.label == "2026-04").unwrap();
        assert!(april.suppressed && april.value.is_none());
        assert!(an.values_withheld && an.mermaid_chart.is_none());
        assert!(an.rows.iter().all(|x| x.center.is_none()));
        assert!(an.phases.iter().all(|p| p.center.is_none()));
        assert!(r.privacy.suppression_applied && r.privacy.suppression_threshold == Some(5));
        assert!(an.chart.as_ref().unwrap().alt_text.contains("chart file"));
        // Identifiers in the file are reported, but nothing from them is returned.
        let all = serde_json::to_string(&r).unwrap() + &r.summary;
        for s in ["Odalys", "Fennimore", "Pemberwick", "413-555-0188", "example.org"] {
            assert!(!all.contains(s), "{s} leaked");
        }
        assert!(r.privacy.identifiers_detected.iter().any(|k| k.to_lowercase().contains("phone")));
        // The tracker and chart are written, and the DOCX omits the chart image.
        assert!(r.data.files.iter().any(|f| f.ends_with(".xlsx")));
        assert!(r.data.files.iter().any(|f| f.ends_with(".docx")));
        for f in &r.data.files {
            assert!(dir.path().join("data").join(f).exists());
        }
    }

    #[test]
    fn identifier_columns_are_refused() {
        let (_d, ctx) = ctx();
        for col in ["Contact Person", "Contact Cell", "Contact Email"] {
            let a = QiPlanArgs {
                path: Some("qi_clinic_measure.csv".into()),
                period_column: Some("Month".into()),
                numerator_column: Some(col.into()),
                sections: vec![Section::Analysis],
                ..Default::default()
            };
            let e = plan_quality_improvement(&ctx, a).unwrap_err().to_string();
            assert!(!e.contains("Odalys") && !e.contains("555"), "{e}");
        }
        let a = QiPlanArgs {
            path: Some("qi_clinic_measure.csv".into()),
            period_column: Some("Contact Person".into()),
            numerator_column: Some("Children Seen".into()),
            sections: vec![Section::Analysis],
            ..Default::default()
        };
        assert!(plan_quality_improvement(&ctx, a).is_err());
    }

    #[test]
    fn contact_details_in_the_request_are_reported_not_hidden() {
        let (_d, ctx) = ctx();
        let mut a = full_args();
        a.measures[0].owner = Some("Call Pat at 413-555-0199".into());
        let r = plan_quality_improvement(&ctx, a).unwrap();
        assert!(r.privacy.identifiers_detected.iter().any(|k| k.to_lowercase().contains("phone")));
        assert!(r.privacy.notes.iter().any(|n| n.contains("roles")));
    }
}
