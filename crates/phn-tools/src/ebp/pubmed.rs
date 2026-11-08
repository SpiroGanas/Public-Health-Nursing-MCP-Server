//! EBP-01 `search_pubmed`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::eutils::{Article, Eutils};
use phn_data::socrata::encode;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// One PICO element.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct PicoElement {
    /// The main phrase, e.g. "school-age children" or "text message reminders".
    pub phrase: String,
    /// Other words or phrases for the same idea, searched in titles and abstracts
    /// (a trailing * truncates, e.g. "vaccin*").
    #[serde(default)]
    pub synonyms: Vec<String>,
    /// MeSH headings for this idea, e.g. "Immunization Schedule". Each is checked
    /// against the MeSH database; headings that do not exist are dropped and reported.
    #[serde(default)]
    pub mesh: Vec<String>,
}

/// Study-type and other filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PubmedFilter {
    /// PubMed's systematic review filter (`systematic[sb]`).
    SystematicReview,
    /// Publication type Meta-Analysis.
    MetaAnalysis,
    /// Publication type Randomized Controlled Trial.
    RandomizedControlledTrial,
    /// Publication types Guideline or Practice Guideline.
    Guideline,
    /// Exclude records indexed only as animal studies.
    Humans,
}

/// Sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    /// PubMed's Best Match (default).
    #[default]
    Relevance,
    /// Newest first.
    Newest,
}

/// Arguments for `search_pubmed`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchPubmedArgs {
    /// P: population or problem.
    #[serde(default)]
    pub population: Option<PicoElement>,
    /// I: intervention or exposure.
    #[serde(default)]
    pub intervention: Option<PicoElement>,
    /// C: comparison. Not added to the query unless include_comparison is true
    /// (most PICO searches leave it out so relevant studies are not missed).
    #[serde(default)]
    pub comparison: Option<PicoElement>,
    /// O: outcome.
    #[serde(default)]
    pub outcome: Option<PicoElement>,
    #[serde(default)]
    pub include_comparison: Option<bool>,
    /// A ready-made PubMed query to run instead of (or in addition to) PICO elements.
    #[serde(default)]
    pub query: Option<String>,
    /// Study-type filters; several are combined with OR.
    #[serde(default)]
    pub filters: Option<Vec<PubmedFilter>>,
    /// Earliest publication year.
    #[serde(default)]
    pub from_year: Option<u32>,
    /// Latest publication year.
    #[serde(default)]
    pub to_year: Option<u32>,
    /// Languages, e.g. ["english", "spanish"] (PubMed language names).
    #[serde(default)]
    pub languages: Option<Vec<String>>,
    /// Records to return, 1–50 (default 10).
    #[serde(default)]
    pub max_results: Option<u32>,
    /// relevance (default) or newest.
    #[serde(default)]
    pub sort: Option<SortOrder>,
}

/// One article.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ArticleSummary {
    pub pmid: String,
    pub title: String,
    pub journal: String,
    pub year: Option<String>,
    /// First three authors, then "et al.".
    pub authors: String,
    /// The first ~600 characters of the abstract.
    pub abstract_excerpt: String,
    pub doi: Option<String>,
    pub url: String,
    pub publication_types: Vec<String>,
}

/// How a PICO element was searched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConceptReport {
    pub element: String,
    pub mesh_used: Vec<String>,
    /// MeSH headings given but not found in MeSH.
    pub mesh_rejected: Vec<String>,
    pub free_text: Vec<String>,
    pub clause: String,
}

/// Result of `search_pubmed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PubmedResult {
    /// The exact query sent to PubMed (paste it into PubMed to rerun).
    pub query: String,
    /// PubMed's own translation of the query.
    pub query_translation: String,
    /// Link that reruns the search on pubmed.ncbi.nlm.nih.gov.
    pub pubmed_url: String,
    pub total_found: u64,
    pub concepts: Vec<ConceptReport>,
    pub articles: Vec<ArticleSummary>,
    pub warnings: Vec<String>,
}

fn quote(term: &str) -> String {
    format!("\"{}\"", term.trim().replace('"', ""))
}

/// The OR-clause for one concept.
pub fn concept_clause(mesh: &[String], free_text: &[String]) -> String {
    let mut parts: Vec<String> = mesh
        .iter()
        .map(|m| format!("{}[MeSH Terms]", quote(m)))
        .collect();
    parts.extend(free_text.iter().map(|t| format!("{}[tiab]", quote(t))));
    format!("({})", parts.join(" OR "))
}

/// The filter clauses (study types, humans, languages, dates).
pub fn filter_clauses(
    filters: &[PubmedFilter],
    languages: &[String],
    from_year: Option<u32>,
    to_year: Option<u32>,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut types = Vec::new();
    for f in filters {
        match f {
            PubmedFilter::SystematicReview => types.push("systematic[sb]".to_string()),
            PubmedFilter::MetaAnalysis => types.push("\"meta-analysis\"[pt]".into()),
            PubmedFilter::RandomizedControlledTrial => {
                types.push("\"randomized controlled trial\"[pt]".into())
            }
            PubmedFilter::Guideline => {
                types.push("\"guideline\"[pt]".into());
                types.push("\"practice guideline\"[pt]".into());
            }
            PubmedFilter::Humans => {}
        }
    }
    types.dedup();
    if !types.is_empty() {
        out.push(format!("({})", types.join(" OR ")));
    }
    if !languages.is_empty() {
        let l: Vec<String> = languages
            .iter()
            .map(|l| format!("{}[lang]", l.trim().to_lowercase()))
            .collect();
        out.push(format!("({})", l.join(" OR ")));
    }
    if from_year.is_some() || to_year.is_some() {
        out.push(format!(
            "(\"{}/01/01\"[dp] : \"{}/12/31\"[dp])",
            from_year.unwrap_or(1800),
            to_year.unwrap_or(3000)
        ));
    }
    out
}

fn summarize(a: &Article) -> ArticleSummary {
    let authors = if a.authors.len() > 3 {
        format!("{}, et al.", a.authors[..3].join(", "))
    } else {
        a.authors.join(", ")
    };
    let mut excerpt: String = a.abstract_text.chars().take(600).collect();
    if a.abstract_text.chars().count() > 600 {
        if let Some(i) = excerpt.rfind(' ') {
            excerpt.truncate(i);
        }
        excerpt.push('…');
    }
    ArticleSummary {
        pmid: a.pmid.clone(),
        title: a.title.clone(),
        journal: if a.journal_abbreviation.is_empty() {
            a.journal.clone()
        } else {
            a.journal_abbreviation.clone()
        },
        year: a.year.clone(),
        authors,
        abstract_excerpt: excerpt,
        doi: a.doi.clone(),
        url: a.url(),
        publication_types: a.publication_types.clone(),
    }
}

/// PICO-based PubMed search.
pub async fn search_pubmed(ctx: &Context, args: SearchPubmedArgs) -> Result<Envelope<PubmedResult>> {
    let max = args.max_results.unwrap_or(10);
    if !(1..=50).contains(&max) {
        return Err(PhnError::InvalidInput("max_results must be between 1 and 50".into()));
    }
    if let (Some(a), Some(b)) = (args.from_year, args.to_year)
        && a > b
    {
        return Err(PhnError::InvalidInput("from_year is after to_year".into()));
    }
    let mut elements: Vec<(&str, &PicoElement)> = Vec::new();
    for (name, e) in [
        ("population", &args.population),
        ("intervention", &args.intervention),
        ("comparison", &args.comparison),
        ("outcome", &args.outcome),
    ] {
        if let Some(e) = e {
            if name == "comparison" && !args.include_comparison.unwrap_or(false) {
                continue;
            }
            if e.phrase.trim().is_empty() && e.synonyms.is_empty() && e.mesh.is_empty() {
                continue;
            }
            elements.push((name, e));
        }
    }
    let raw = args.query.as_deref().map(str::trim).filter(|q| !q.is_empty());
    if elements.is_empty() && raw.is_none() {
        return Err(PhnError::InvalidInput(
            "give at least one PICO element (population, intervention, outcome) or a query".into(),
        ));
    }
    if ctx.http.is_offline() {
        return Err(PhnError::Network(
            "PubMed searches need network access, and offline mode is on (PHN_OFFLINE). Run the \
             search at https://pubmed.ncbi.nlm.nih.gov/ instead."
                .into(),
        ));
    }
    let eu = Eutils::new();
    let cache = ctx.cache.as_ref();
    let mut concepts = Vec::new();
    let mut clauses = Vec::new();
    let mut warnings = Vec::new();
    for (name, e) in &elements {
        let mut mesh_used = Vec::new();
        let mut mesh_rejected = Vec::new();
        for m in &e.mesh {
            if eu.valid_mesh(&ctx.http, cache, m).await? {
                mesh_used.push(m.trim().to_string());
            } else {
                mesh_rejected.push(m.trim().to_string());
            }
        }
        if !e.phrase.trim().is_empty() {
            for m in eu.mesh_for_phrase(&ctx.http, cache, e.phrase.trim()).await? {
                if !mesh_used.iter().any(|x: &String| x.eq_ignore_ascii_case(&m)) {
                    mesh_used.push(m);
                }
            }
        }
        let mut free_text: Vec<String> = Vec::new();
        for t in std::iter::once(&e.phrase).chain(e.synonyms.iter()) {
            let t = t.trim();
            if !t.is_empty() && !free_text.iter().any(|x| x.eq_ignore_ascii_case(t)) {
                free_text.push(t.to_string());
            }
        }
        let clause = concept_clause(&mesh_used, &free_text);
        if !mesh_rejected.is_empty() {
            warnings.push(format!(
                "{name}: not MeSH headings, dropped: {}",
                mesh_rejected.join(", ")
            ));
        }
        clauses.push(clause.clone());
        concepts.push(ConceptReport {
            element: name.to_string(),
            mesh_used,
            mesh_rejected,
            free_text,
            clause,
        });
    }
    if let Some(q) = raw {
        clauses.push(format!("({q})"));
    }
    let filters = args.filters.clone().unwrap_or_default();
    let languages = args.languages.clone().unwrap_or_default();
    clauses.extend(filter_clauses(&filters, &languages, args.from_year, args.to_year));
    let mut query = clauses.join(" AND ");
    if filters.contains(&PubmedFilter::Humans) {
        query = format!("({query}) NOT (\"animals\"[MeSH Terms] NOT \"humans\"[MeSH Terms])");
    }
    let sort = match args.sort.unwrap_or_default() {
        SortOrder::Relevance => "relevance",
        SortOrder::Newest => "pub_date",
    };
    let (found, prov, _) = eu.search(&ctx.http, cache, &query, max, sort).await?;
    warnings.extend(found.warnings.iter().cloned());
    let (articles, _) = eu.fetch(&ctx.http, cache, &found.ids).await?;
    let articles: Vec<ArticleSummary> = articles.iter().map(summarize).collect();
    let pubmed_url = format!("https://pubmed.ncbi.nlm.nih.gov/?term={}", encode(&query));

    let mut summary = format!(
        "**PubMed search** — {} record(s) found; showing {} ({}).\n\nQuery (paste into PubMed to rerun):\n\n```\n{query}\n```\n\n",
        md::num(found.count as f64, 0),
        articles.len(),
        if sort == "pub_date" { "newest first" } else { "best match" }
    );
    if !concepts.is_empty() {
        let rows: Vec<Vec<String>> = concepts
            .iter()
            .map(|c| {
                vec![
                    c.element.clone(),
                    if c.mesh_used.is_empty() { "—".into() } else { c.mesh_used.join("; ") },
                    c.free_text.join("; "),
                ]
            })
            .collect();
        summary.push_str(&md::table(&["PICO element", "MeSH headings", "Title/abstract terms"], &rows));
        summary.push('\n');
    }
    for (i, a) in articles.iter().enumerate() {
        summary.push_str(&format!(
            "{}. **{}** {}. *{}* {}. PMID [{}]({}){}{}\n",
            i + 1,
            a.title,
            a.authors,
            a.journal,
            a.year.clone().unwrap_or_default(),
            a.pmid,
            a.url,
            a.doi.as_ref().map(|d| format!("; doi:{d}")).unwrap_or_default(),
            {
                let types: Vec<&str> = a
                    .publication_types
                    .iter()
                    .map(String::as_str)
                    .filter(|t| *t != "Journal Article" && !t.starts_with("Research Support"))
                    .collect();
                if types.is_empty() { String::new() } else { format!(" — {}", types.join(", ")) }
            }
        ));
        if !a.abstract_excerpt.is_empty() {
            summary.push_str(&format!("   > {}\n", a.abstract_excerpt));
        }
    }
    let mut caveats = vec![
        "Abstract excerpts are not appraisals; read the full text and appraise study quality \
         (for example with the JHNEBP or CASP tools) before applying findings."
            .to_string(),
        "The newest PubMed records may not have MeSH headings yet; the title/abstract terms in \
         the query catch them."
            .to_string(),
    ];
    if !warnings.is_empty() {
        caveats.push(format!("PubMed notes: {}", warnings.join("; ")));
    }
    let source = Source {
        title: "PubMed (NCBI E-utilities esearch and efetch)".into(),
        publisher: "National Library of Medicine, National Center for Biotechnology Information".into(),
        url: Some(pubmed_url.clone()),
        effective_date: None,
        retrieved: Some(prov.as_of.chars().take(10).collect()),
        mode: prov.mode,
    };
    Ok(Envelope::new(
        summary,
        PubmedResult {
            query,
            query_translation: found.query_translation,
            pubmed_url,
            total_found: found.count,
            concepts,
            articles,
            warnings,
        },
    )
    .source(source)
    .caveats(caveats)
    .privacy(Privacy::none()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_pico_clauses_and_filters() {
        let c = concept_clause(
            &["Whooping Cough".into()],
            &["pertussis".into(), "vaccin*".into()],
        );
        assert_eq!(
            c,
            "(\"Whooping Cough\"[MeSH Terms] OR \"pertussis\"[tiab] OR \"vaccin*\"[tiab])"
        );
        let f = filter_clauses(
            &[PubmedFilter::SystematicReview, PubmedFilter::Guideline],
            &["English".into()],
            Some(2015),
            None,
        );
        assert_eq!(
            f,
            vec![
                "(systematic[sb] OR \"guideline\"[pt] OR \"practice guideline\"[pt])",
                "(english[lang])",
                "(\"2015/01/01\"[dp] : \"3000/12/31\"[dp])"
            ]
        );
    }

    #[tokio::test]
    async fn offline_is_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let err = search_pubmed(
            &ctx,
            SearchPubmedArgs {
                population: Some(PicoElement {
                    phrase: "adolescents".into(),
                    ..PicoElement::default()
                }),
                ..SearchPubmedArgs::default()
            },
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(err.contains("offline"), "{err}");
        let err = search_pubmed(&ctx, SearchPubmedArgs::default()).await.unwrap_err().to_string();
        assert!(err.contains("PICO"), "{err}");
    }
}
