//! VAX-02 `compare_vaccine_recommendations`.

use phn_core::text;
use phn_core::{
    Envelope, PhnError, Privacy, Result, Source, SourceMode, VaccineRecommendationBasis,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;
use crate::vax::content::{self, Body, Population, Position, RECS, basis_order};

/// Arguments for `compare_vaccine_recommendations`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CompareArgs {
    /// Vaccine, disease, or question, e.g. "hepatitis B birth dose", "MMRV",
    /// "COVID pregnancy", "flu for kids", "which childhood schedule". Omit to
    /// list every topic and whether the bodies differ.
    pub query: Option<String>,
    /// Limit to a population: infants, children, adolescents, adults, or pregnancy.
    pub population: Option<String>,
    /// Override the saved basis for this answer: dph_then_professional_then_cdc,
    /// cdc_acip, professional_societies, or show_all.
    pub basis: Option<String>,
}

/// One topic compared across bodies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TopicComparison {
    pub id: String,
    pub title: String,
    pub vaccine: String,
    pub population: Population,
    /// Positions ordered by the basis (the first is the basis unless show_all).
    pub positions: Vec<Position>,
    /// The body whose position is the basis (none with show_all).
    pub basis_body: Option<Body>,
    /// True when the bodies' positions differ.
    pub differs: bool,
    pub difference: String,
    /// Bodies with no position found in the reviewed documents.
    pub bodies_without_position: Vec<Body>,
}

/// Result of `compare_vaccine_recommendations`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CompareResult {
    pub basis: String,
    pub basis_description: String,
    pub topics: Vec<TopicComparison>,
    /// Background on the 2025-2026 divergence.
    pub context: Vec<String>,
}

fn population_filter(p: &str) -> Result<Vec<Population>> {
    let t = p.trim().to_ascii_lowercase();
    Ok(match t.as_str() {
        "infant" | "infants" | "newborn" | "newborns" => vec![
            Population::Infants,
            Population::Children,
            Population::ChildrenAdolescents,
        ],
        "child" | "children" | "kids" | "pediatric" => vec![
            Population::Infants,
            Population::Children,
            Population::ChildrenAdolescents,
        ],
        "adolescent" | "adolescents" | "teen" | "teens" => {
            vec![Population::Adolescents, Population::ChildrenAdolescents]
        }
        "adult" | "adults" | "older adults" => vec![Population::Adults],
        "pregnancy" | "pregnant" | "prenatal" => vec![Population::Pregnancy],
        _ => {
            return Err(PhnError::InvalidInput(format!(
                "population '{p}' is not recognized; use infants, children, adolescents, adults, or pregnancy"
            )));
        }
    })
}

/// Compare recommendations from DPH, CDC/ACIP, AAP, ACOG, AAFP, and NEPHC.
pub fn compare_vaccine_recommendations(
    ctx: &Context,
    args: CompareArgs,
) -> Result<Envelope<CompareResult>> {
    let basis = match &args.basis {
        Some(b) => b
            .parse::<VaccineRecommendationBasis>()
            .map_err(PhnError::InvalidInput)?,
        None => ctx
            .settings()
            .vaccine_recommendation_basis
            .unwrap_or_default(),
    };
    let pops = args
        .population
        .as_deref()
        .map(population_filter)
        .transpose()?;
    let query = args
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty());

    let mut indices: Vec<usize> = match query {
        Some(q) => {
            let ranked = text::rank(
                q,
                &RECS.topics,
                |t| {
                    let mut k = vec![t.title.clone(), t.vaccine.clone(), t.id.replace('_', " ")];
                    k.extend(t.aliases.iter().cloned());
                    k
                },
                0.80,
            );
            let top = ranked.first().map(|r| r.score).unwrap_or(0.0);
            ranked
                .into_iter()
                .filter(|r| r.score >= top - 0.05)
                .map(|r| r.index)
                .collect()
        }
        None => (0..RECS.topics.len()).collect(),
    };
    if let Some(p) = &pops {
        indices.retain(|&i| {
            p.contains(&RECS.topics[i].population) || RECS.topics[i].population == Population::All
        });
    }
    if indices.is_empty() {
        let list: Vec<String> = RECS.topics.iter().map(|t| t.title.clone()).collect();
        return Err(PhnError::NotFound(format!(
            "no recommendation topic matches{}. Topics: {}",
            query.map(|q| format!(" '{q}'")).unwrap_or_default(),
            list.join("; ")
        )));
    }

    let all_bodies = [Body::Dph, Body::CdcAcip, Body::Aap, Body::Acog, Body::Aafp];
    let topics: Vec<TopicComparison> = indices
        .iter()
        .map(|&i| {
            let t = &RECS.topics[i];
            let order = basis_order(basis, t.population);
            let mut positions = t.positions.clone();
            positions.sort_by_key(|p| {
                order
                    .iter()
                    .position(|b| *b == p.body)
                    .unwrap_or(usize::MAX)
            });
            let basis_body = (basis != VaccineRecommendationBasis::ShowAll)
                .then(|| positions.first().map(|p| p.body))
                .flatten();
            let present: Vec<Body> = positions.iter().map(|p| p.body).collect();
            let relevant: Vec<Body> = all_bodies
                .iter()
                .copied()
                .filter(|b| match b {
                    Body::Acog => t.population == Population::Pregnancy,
                    Body::Aafp => matches!(
                        t.population,
                        Population::Adults | Population::Pregnancy | Population::All
                    ),
                    Body::Aap => {
                        !matches!(t.population, Population::Adults | Population::Pregnancy)
                    }
                    _ => true,
                })
                .collect();
            TopicComparison {
                id: t.id.clone(),
                title: t.title.clone(),
                vaccine: t.vaccine.clone(),
                population: t.population,
                bodies_without_position: relevant
                    .into_iter()
                    .filter(|b| !present.contains(b))
                    .collect(),
                positions,
                basis_body,
                differs: t.differs,
                difference: t.difference.clone(),
            }
        })
        .collect();

    let detail = query.is_some() && topics.len() <= 3;
    let mut summary = format!(
        "**Vaccine recommendations compared** (basis: {})\n\n",
        basis.describe()
    );
    if detail {
        for t in &topics {
            summary.push_str(&format!("### {}\n", t.title));
            if let Some(b) = t.basis_body
                && let Some(p) = t.positions.first()
            {
                summary.push_str(&format!("**Basis ({}):** {}\n\n", b.short(), p.summary));
            }
            let rows: Vec<Vec<String>> = t
                .positions
                .iter()
                .map(|p| {
                    vec![
                        p.body.short().to_string(),
                        p.summary.clone(),
                        p.date.clone().unwrap_or_else(|| "not stated".into()),
                        format!("{:?}", p.confidence).to_lowercase(),
                    ]
                })
                .collect();
            summary.push_str(&md::table(
                &["Body", "Position", "Date", "Confidence"],
                &rows,
            ));
            summary.push_str(&format!(
                "\n{} {}\n",
                if t.differs {
                    "⚠ **The bodies differ.**"
                } else {
                    "The bodies agree."
                },
                t.difference
            ));
            if !t.bodies_without_position.is_empty() {
                summary.push_str(&format!(
                    "No position found in the reviewed documents for: {}.\n",
                    t.bodies_without_position
                        .iter()
                        .map(|b| b.short())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            summary.push('\n');
        }
    } else {
        let rows: Vec<Vec<String>> = topics
            .iter()
            .map(|t| {
                vec![
                    t.title.clone(),
                    t.population.label().to_string(),
                    if t.differs {
                        "differ".into()
                    } else {
                        "agree".into()
                    },
                    t.basis_body
                        .map(|b| b.short().to_string())
                        .unwrap_or_else(|| "—".into()),
                ]
            })
            .collect();
        summary.push_str(&md::table(
            &["Topic", "Population", "Bodies", "Basis"],
            &rows,
        ));
        summary.push_str("\nAsk about one topic for each body's position, date, and source.\n");
    }

    let mut sources: Vec<Source> = vec![content::content_source(content::RECOMMENDATIONS)?];
    let retrieved = content::meta(content::RECOMMENDATIONS)
        .ok()
        .map(|m| m.prepared);
    for t in &topics {
        for p in &t.positions {
            if sources
                .iter()
                .any(|s| s.url.as_deref() == Some(p.source.url.as_str()))
            {
                continue;
            }
            sources.push(Source {
                title: p.source.title.clone(),
                publisher: p.source.publisher.clone(),
                url: Some(p.source.url.clone()),
                effective_date: p.date.clone(),
                retrieved: retrieved.clone(),
                mode: SourceMode::Snapshot,
            });
        }
    }
    let mut caveats: Vec<String> = Vec::new();
    if topics.iter().any(|t| t.differs) {
        caveats.push(
            "Federal (CDC/ACIP) recommendations differ from Massachusetts DPH and the professional \
             societies on at least one topic shown. Massachusetts school requirements follow DPH \
             (aligned with AAP). Tell families which recommendation you are following and why."
                .into(),
        );
    }
    if topics
        .iter()
        .flat_map(|t| &t.positions)
        .any(|p| p.confidence != content::Confidence::High)
    {
        caveats.push(
            "Positions marked medium confidence come from a document that describes the body's \
             position rather than the body's own document; check the original before relying on it."
                .into(),
        );
    }
    caveats.push(
        "Recommendations change often in 2025-2026. Check the date of each position and the issuing \
         body's site for anything newer."
            .into(),
    );
    caveats.extend(content::review_caveats(&[content::RECOMMENDATIONS]));
    Ok(Envelope::new(
        summary,
        CompareResult {
            basis: basis.as_str().into(),
            basis_description: basis.describe().into(),
            topics,
            context: RECS.context.clone(),
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

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    fn topic(ctx: &Context, query: &str, basis: Option<&str>) -> TopicComparison {
        compare_vaccine_recommendations(
            ctx,
            CompareArgs {
                query: Some(query.into()),
                population: None,
                basis: basis.map(String::from),
            },
        )
        .unwrap()
        .data
        .topics
        .remove(0)
    }

    #[test]
    fn basis_orders_positions_by_population() {
        let (_d, ctx) = ctx();
        // Pregnancy: DPH first, then ACOG ahead of the other societies.
        let t = topic(&ctx, "COVID-19 vaccine during pregnancy", None);
        assert_eq!(t.id, "covid19_pregnancy");
        let order: Vec<Body> = t.positions.iter().map(|p| p.body).collect();
        let pos = |b: Body| order.iter().position(|x| *x == b);
        assert_eq!(order[0], Body::Dph);
        if let (Some(acog), Some(aap)) = (pos(Body::Acog), pos(Body::Aap)) {
            assert!(acog < aap, "{order:?}");
        }
        assert_eq!(order.last(), Some(&Body::CdcAcip));
        // CDC/ACIP first when chosen.
        let t = topic(&ctx, "hepatitis B birth dose", Some("cdc_acip"));
        assert_eq!(t.basis_body, Some(Body::CdcAcip));
        // Professional societies first: AAP for an infant topic.
        let t = topic(
            &ctx,
            "hepatitis B birth dose",
            Some("professional_societies"),
        );
        assert_eq!(t.basis_body, Some(Body::Aap));
    }

    #[test]
    fn every_topic_has_dated_cited_positions() {
        for t in &RECS.topics {
            assert!(!t.positions.is_empty(), "{} has no positions", t.id);
            for p in &t.positions {
                assert!(
                    p.source.url.starts_with("https://"),
                    "{}: {:?}",
                    t.id,
                    p.body
                );
                assert!(!p.summary.trim().is_empty());
            }
        }
    }
}
