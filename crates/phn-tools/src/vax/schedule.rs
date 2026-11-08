//! VAX-01 `get_immunization_schedule`.

use phn_core::text;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;
use crate::vax::content::{self, Body, Population, Routine, SCHED, SeriesDose, basis_order};

/// Arguments for `get_immunization_schedule`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ScheduleArgs {
    /// A vaccine or disease (e.g. "MMR", "hep B", "Shingrix", "HPV"). Omit for
    /// an overview of every vaccine with its routine schedule.
    pub vaccine: Option<String>,
    /// "child" (birth through 18, the default) or "adult". Controls which
    /// professional society is consulted first and which editions are listed.
    pub population: Option<String>,
    /// Include minimum ages and intervals between doses (default true when a
    /// vaccine is named).
    pub include_minimums: Option<bool>,
    /// Include contraindications and precautions (default true when a vaccine
    /// is named).
    pub include_contraindications: Option<bool>,
}

/// One dose's minimum age and interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SeriesTable {
    pub series_id: String,
    pub label: String,
    pub doses: Vec<SeriesDose>,
    pub notes: Vec<String>,
}

/// One vaccine in the answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VaccineSchedule {
    pub id: String,
    pub name: String,
    pub abbreviations: Vec<String>,
    /// Routine recommendations, ordered by the configured basis (first = basis).
    pub routine: Vec<Routine>,
    pub minimums: Vec<SeriesTable>,
    pub contraindications: Vec<String>,
    pub precautions: Vec<String>,
    pub confidence: content::Confidence,
}

/// A schedule edition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EditionInfo {
    pub body: Body,
    pub title: String,
    pub date: Option<String>,
    pub url: String,
    pub note: String,
}

/// Result of `get_immunization_schedule`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScheduleResult {
    /// Recommendation basis in force (see configure_jurisdiction).
    pub basis: String,
    pub population: String,
    pub editions: Vec<EditionInfo>,
    pub vaccines: Vec<VaccineSchedule>,
    /// General rules: grace period, live-vaccine spacing, combination vaccines.
    pub general_rules: Vec<String>,
}

fn population_arg(p: Option<&str>) -> Result<Population> {
    match p.map(|s| s.trim().to_ascii_lowercase()) {
        None => Ok(Population::ChildrenAdolescents),
        Some(s) if s.is_empty() || s.starts_with("child") || s.starts_with("ped") || s == "kids" => {
            Ok(Population::ChildrenAdolescents)
        }
        Some(s) if s.starts_with("adult") => Ok(Population::Adults),
        Some(s) => Err(PhnError::InvalidInput(format!(
            "population '{s}' is not recognized; use \"child\" or \"adult\""
        ))),
    }
}

fn order_routine(routine: &[Routine], order: &[Body]) -> Vec<Routine> {
    let mut r = routine.to_vec();
    r.sort_by_key(|x| order.iter().position(|b| *b == x.body).unwrap_or(usize::MAX));
    r
}

/// Look up a vaccine entry by name, abbreviation, or alias.
pub fn find_vaccine(query: &str) -> Vec<&'static content::VaccineEntry> {
    let ranked = text::rank(
        query,
        &SCHED.vaccines,
        |v| {
            let mut k = vec![v.name.clone(), v.id.clone()];
            k.extend(v.abbreviations.iter().cloned());
            k.extend(v.aliases.iter().cloned());
            k
        },
        0.82,
    );
    let top = ranked.first().map(|r| r.score).unwrap_or(0.0);
    ranked
        .into_iter()
        .filter(|r| r.score >= top - 0.02)
        .map(|r| &SCHED.vaccines[r.index])
        .collect()
}

/// Return schedules with edition dates, minimum ages and intervals, and
/// contraindications and precautions.
pub fn get_immunization_schedule(
    ctx: &Context,
    args: ScheduleArgs,
) -> Result<Envelope<ScheduleResult>> {
    let population = population_arg(args.population.as_deref())?;
    let basis = ctx.settings().vaccine_recommendation_basis.unwrap_or_default();
    let order = basis_order(basis, population);
    let named = args.vaccine.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let entries: Vec<&content::VaccineEntry> = match named {
        Some(q) => {
            let found = find_vaccine(q);
            if found.is_empty() {
                let names: Vec<String> = SCHED.vaccines.iter().map(|v| v.name.clone()).collect();
                return Err(PhnError::NotFound(format!(
                    "no vaccine matches '{q}'. Known vaccines: {}",
                    names.join("; ")
                )));
            }
            found
        }
        None => SCHED.vaccines.iter().collect(),
    };
    let detail = named.is_some();
    let include_min = args.include_minimums.unwrap_or(detail);
    let include_contra = args.include_contraindications.unwrap_or(detail);

    let vaccines: Vec<VaccineSchedule> = entries
        .iter()
        .map(|v| VaccineSchedule {
            id: v.id.clone(),
            name: v.name.clone(),
            abbreviations: v.abbreviations.clone(),
            routine: order_routine(&v.routine, &order),
            minimums: if include_min {
                v.series
                    .iter()
                    .filter_map(|s| SCHED.series(s))
                    .map(|s| SeriesTable {
                        series_id: s.id.clone(),
                        label: s.label.clone(),
                        doses: s.doses.clone(),
                        notes: s.notes.clone(),
                    })
                    .collect()
            } else {
                Vec::new()
            },
            contraindications: if include_contra { v.contraindications.clone() } else { Vec::new() },
            precautions: if include_contra { v.precautions.clone() } else { Vec::new() },
            confidence: v.confidence,
        })
        .collect();

    let pop_key = if population == Population::Adults { "adults" } else { "children_adolescents" };
    let mut editions: Vec<EditionInfo> = SCHED
        .editions
        .iter()
        .filter(|e| e.population == pop_key)
        .map(|e| EditionInfo {
            body: e.body,
            title: e.title.clone(),
            date: e.date.clone(),
            url: e.url.clone(),
            note: e.note.clone(),
        })
        .collect();
    editions.sort_by_key(|e| order.iter().position(|b| *b == e.body).unwrap_or(usize::MAX));

    let mut summary = format!(
        "**Immunization schedule — {}** (basis: {})\n\n",
        population.label(),
        basis.describe()
    );
    summary.push_str("**Editions in use**\n");
    for e in &editions {
        summary.push_str(&format!(
            "- {}: {} ({})\n",
            e.body.short(),
            e.title,
            e.date.clone().unwrap_or_else(|| "date not stated".into())
        ));
    }
    summary.push('\n');
    for v in &vaccines {
        summary.push_str(&format!("### {}\n", v.name));
        for (i, r) in v.routine.iter().enumerate() {
            let tag = if i == 0 && basis != phn_core::VaccineRecommendationBasis::ShowAll {
                " (basis)"
            } else {
                ""
            };
            summary.push_str(&format!("- **{}{tag}:** {}\n", r.body.short(), r.text));
        }
        for t in &v.minimums {
            summary.push_str(&format!("\nMinimum ages and intervals — {}\n\n", t.label));
            let rows: Vec<Vec<String>> = t
                .doses
                .iter()
                .map(|d| {
                    vec![
                        format!("{}", d.dose),
                        d.recommended_age.clone().unwrap_or_default(),
                        d.min_age.clone(),
                        d.min_interval.clone().unwrap_or_else(|| "—".into()),
                    ]
                })
                .collect();
            summary.push_str(&md::table(
                &["Dose", "Recommended age", "Minimum age", "Minimum interval from previous dose"],
                &rows,
            ));
            for n in &t.notes {
                summary.push_str(&format!("- {n}\n"));
            }
        }
        if !v.contraindications.is_empty() {
            summary.push_str("\n**Contraindications:** ");
            summary.push_str(&v.contraindications.join(" "));
            summary.push('\n');
        }
        if !v.precautions.is_empty() {
            summary.push_str("**Precautions:** ");
            summary.push_str(&v.precautions.join(" "));
            summary.push('\n');
        }
        summary.push('\n');
    }
    if !detail {
        summary.push_str(
            "Ask about one vaccine for minimum ages, intervals, contraindications, and precautions.\n",
        );
    }

    let general_rules: Vec<String> = SCHED.general_rules.iter().map(|r| r.text.clone()).collect();
    let mut caveats = vec![
        "Federal (CDC/ACIP) guidance has differed from DPH, AAP, ACOG, and AAFP since 2025. DPH \
         recommends the AAP schedule for children; use compare_vaccine_recommendations to see each \
         body's position with dates."
            .to_string(),
        "Doses given 4 or fewer days before a minimum age or interval are valid (CDC grace period); \
         5 or more days early are not and should be repeated."
            .to_string(),
        "Check the package insert for product-specific minimum ages and use \
         forecast_immunizations_due (when available) or the MIIS forecaster for catch-up schedules."
            .to_string(),
    ];
    if vaccines.iter().any(|v| v.confidence != content::Confidence::High) {
        caveats.push(
            "Some entries are marked medium confidence because they were drafted from secondary \
             sources (the AAP schedule PDF could not be downloaded)."
                .into(),
        );
    }
    caveats.extend(content::review_caveats(&[content::SCHEDULE]));

    let mut sources = vec![content::content_source(content::SCHEDULE)?];
    for e in &editions {
        sources.push(Source {
            title: e.title.clone(),
            publisher: match e.body {
                Body::Aap => "American Academy of Pediatrics",
                Body::Aafp => "American Academy of Family Physicians",
                Body::CdcAcip => "Centers for Disease Control and Prevention",
                Body::Dph => "Massachusetts Department of Public Health",
                Body::Acog => "American College of Obstetricians and Gynecologists",
                Body::Nephc => "Northeast Public Health Collaborative",
            }
            .into(),
            url: Some(e.url.clone()),
            effective_date: e.date.clone(),
            retrieved: content::meta(content::SCHEDULE).ok().map(|m| m.prepared),
            mode: SourceMode::Snapshot,
        });
    }
    Ok(Envelope::new(
        summary,
        ScheduleResult {
            basis: basis.as_str().into(),
            population: population.label().into(),
            editions,
            vaccines,
            general_rules,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy::none())
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_vaccines_by_alias() {
        assert_eq!(find_vaccine("Shingrix")[0].id, "zoster");
        assert_eq!(find_vaccine("hep b")[0].id, "hepb");
        assert_eq!(find_vaccine("chickenpox vaccine")[0].id, "varicella");
        assert!(find_vaccine("xyzzy").is_empty());
    }
}
