//! EBP-02 `search_cdc_guidance`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::cdc_guidance::{self, GuidanceItem, Series};
use phn_data::eutils::Eutils;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// Arguments for `search_cdc_guidance`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct CdcGuidanceArgs {
    /// Topic, e.g. "measles", "Candida auris", "lead poisoning".
    pub topic: String,
    /// Series to search (default all): mmwr, han, stacks.
    #[serde(default)]
    pub series: Option<Vec<Series>>,
    /// Only items published on or after this date (YYYY-MM-DD).
    #[serde(default)]
    pub since: Option<String>,
    /// Items to return per series, 1–30 (default 10).
    #[serde(default)]
    pub max_results: Option<usize>,
}

/// How one series was searched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SeriesStatus {
    pub series: Series,
    pub method: String,
    /// ok, failed, or link_only.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub results: usize,
}

/// Result of `search_cdc_guidance`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CdcGuidanceResult {
    pub topic: String,
    /// Newest first.
    pub items: Vec<GuidanceItem>,
    pub searched: Vec<SeriesStatus>,
    /// CDC Stacks search link (newest first) for the nurse to open.
    pub stacks_search_url: String,
}

fn norm_title(t: &str) -> String {
    t.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Search MMWR, HAN, and CDC Stacks for current CDC guidance on a topic.
pub async fn search_cdc_guidance(
    ctx: &Context,
    args: CdcGuidanceArgs,
) -> Result<Envelope<CdcGuidanceResult>> {
    let topic = args.topic.trim().to_string();
    if topic.is_empty() {
        return Err(PhnError::InvalidInput("give a topic to search for".into()));
    }
    let max = args.max_results.unwrap_or(10);
    if !(1..=30).contains(&max) {
        return Err(PhnError::InvalidInput("max_results must be between 1 and 30".into()));
    }
    let since = match &args.since {
        Some(s) => Some(
            chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
                .map_err(|_| PhnError::InvalidInput("since must be a date like 2025-01-31".into()))?
                .to_string(),
        ),
        None => None,
    };
    let series = args
        .series
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| vec![Series::Mmwr, Series::Han, Series::Stacks]);
    let words = cdc_guidance::query_words(&topic);
    let keep = |i: &GuidanceItem| {
        since
            .as_deref()
            .is_none_or(|s| i.date.as_deref().is_some_and(|d| d >= s || (d.len() == 4 && d >= &s[..4])))
    };
    let cache = ctx.cache.as_ref();
    let mut items: Vec<GuidanceItem> = Vec::new();
    let mut searched = Vec::new();
    let mut sources = Vec::new();
    let mut caveats = Vec::new();

    if series.contains(&Series::Mmwr) {
        let mut mmwr: Vec<GuidanceItem> = Vec::new();
        let mut methods = Vec::new();
        let mut errors = Vec::new();
        match cdc_guidance::mmwr(&ctx.http, cache, &topic).await {
            Ok((list, prov)) => {
                methods.push(prov.method.clone());
                sources.push(Source {
                    title: "CDC Content Syndication API (MMWR pages)".into(),
                    publisher: "Centers for Disease Control and Prevention".into(),
                    url: Some(prov.url.clone()),
                    effective_date: None,
                    retrieved: Some(prov.as_of.chars().take(10).collect()),
                    mode: prov.mode,
                });
                mmwr.extend(list);
            }
            Err(e) => errors.push(format!("syndication API: {e}")),
        }
        // PubMed indexes MMWR, which searches abstracts as well as titles.
        if !ctx.http.is_offline() {
            let eu = Eutils::new();
            let q = format!("\"MMWR Morb Mortal Wkly Rep\"[ta] AND ({topic})");
            match eu.search(&ctx.http, cache, &q, max as u32, "pub_date").await {
                Ok((found, prov, _)) => match eu.fetch(&ctx.http, cache, &found.ids).await {
                    Ok((arts, _)) => {
                        methods.push("PubMed search of the MMWR journal (titles and abstracts)".into());
                        sources.push(Source {
                            title: "PubMed: MMWR Morbidity and Mortality Weekly Report".into(),
                            publisher: "National Library of Medicine".into(),
                            url: Some(format!(
                                "https://pubmed.ncbi.nlm.nih.gov/?term={}&sort=date",
                                phn_data::socrata::encode(&q)
                            )),
                            effective_date: None,
                            retrieved: Some(prov.as_of.chars().take(10).collect()),
                            mode: prov.mode,
                        });
                        for a in arts {
                            if mmwr.iter().any(|m| norm_title(&m.title) == norm_title(&a.title)) {
                                continue;
                            }
                            let summary: String = a.abstract_text.chars().take(300).collect();
                            mmwr.push(GuidanceItem {
                                series: Series::Mmwr,
                                title: a.title.trim_end_matches('.').to_string(),
                                date: a.year.clone(),
                                url: a.url(),
                                summary,
                                message_type: None,
                                number: None,
                            });
                        }
                    }
                    Err(e) => errors.push(format!("PubMed: {e}")),
                },
                Err(e) => errors.push(format!("PubMed: {e}")),
            }
        }
        mmwr.retain(|i| keep(i));
        mmwr.sort_by(|a, b| b.date.cmp(&a.date));
        mmwr.truncate(max);
        searched.push(SeriesStatus {
            series: Series::Mmwr,
            method: if methods.is_empty() {
                "CDC Content Syndication API title search; PubMed MMWR journal search".into()
            } else {
                methods.join("; ")
            },
            status: if methods.is_empty() { "failed" } else { "ok" }.into(),
            detail: (!errors.is_empty()).then(|| errors.join("; ")),
            results: mmwr.len(),
        });
        items.extend(mmwr);
    }

    if series.contains(&Series::Han) {
        match cdc_guidance::han_notices(&ctx.http, cache).await {
            Ok((all, prov, notes)) if !all.is_empty() => {
                let mut scored: Vec<(usize, GuidanceItem)> = all
                    .into_iter()
                    .map(|i| (cdc_guidance::match_score(&i, &words), i))
                    .filter(|(s, i)| (*s > 0 || words.is_empty()) && keep(i))
                    .collect();
                // Best match first, then newest.
                scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.date.cmp(&a.1.date)));
                let mut han: Vec<GuidanceItem> = scored.into_iter().map(|x| x.1).take(max).collect();
                han.sort_by(|a, b| b.date.cmp(&a.date));
                sources.push(Source {
                    title: "CDC Health Alert Network (HAN) notices".into(),
                    publisher: "Centers for Disease Control and Prevention".into(),
                    url: Some(prov.url.clone()),
                    effective_date: None,
                    retrieved: Some(prov.as_of.chars().take(10).collect()),
                    mode: prov.mode,
                });
                caveats.extend(notes.iter().cloned());
                searched.push(SeriesStatus {
                    series: Series::Han,
                    method: prov.method,
                    status: "ok".into(),
                    detail: (!notes.is_empty()).then(|| notes.join("; ")),
                    results: han.len(),
                });
                items.extend(han);
            }
            Ok((_, prov, notes)) => searched.push(SeriesStatus {
                series: Series::Han,
                method: prov.method,
                status: "failed".into(),
                detail: Some(notes.join("; ")),
                results: 0,
            }),
            Err(e) => searched.push(SeriesStatus {
                series: Series::Han,
                method: "HAN archive module and notice pages".into(),
                status: "failed".into(),
                detail: Some(e.to_string()),
                results: 0,
            }),
        }
    }

    let stacks_search_url = cdc_guidance::stacks_search_url(&topic);
    if series.contains(&Series::Stacks) {
        searched.push(SeriesStatus {
            series: Series::Stacks,
            method: "search link only".into(),
            status: "link_only".into(),
            detail: Some(
                "CDC Stacks refuses automated searches (HTTP 403) and its OAI-PMH interface has \
                 no topic search, so the server cannot list its results; open the link to search \
                 Stacks, newest first."
                    .into(),
            ),
            results: 0,
        });
    }
    if searched
        .iter()
        .all(|s| s.status != "ok")
        && searched.iter().any(|s| s.status == "failed")
    {
        return Err(PhnError::Network(format!(
            "CDC sources could not be reached ({}). Search CDC Stacks at {stacks_search_url} or \
             MMWR at https://www.cdc.gov/mmwr/.",
            searched
                .iter()
                .filter_map(|s| s.detail.clone())
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    items.sort_by(|a, b| b.date.cmp(&a.date));

    let mut summary = format!("**CDC guidance on \"{topic}\"** (newest first)\n\n");
    for s in [Series::Han, Series::Mmwr] {
        let list: Vec<&GuidanceItem> = items.iter().filter(|i| i.series == s).collect();
        if !series.contains(&s) {
            continue;
        }
        summary.push_str(&format!(
            "**{}**\n\n",
            match s {
                Series::Han => "Health Alert Network notices",
                Series::Mmwr => "MMWR",
                Series::Stacks => "CDC Stacks",
            }
        ));
        if list.is_empty() {
            summary.push_str("No matching items found.\n\n");
            continue;
        }
        let rows: Vec<Vec<String>> = list
            .iter()
            .map(|i| {
                let mut title = i.title.clone();
                if let Some(t) = &i.message_type {
                    title = format!("{title} ({t})");
                }
                if let Some(n) = &i.number {
                    title = format!("HAN {n}: {title}");
                }
                vec![
                    i.date.clone().unwrap_or_default(),
                    format!("[{title}]({})", i.url),
                    i.summary.chars().take(220).collect::<String>(),
                ]
            })
            .collect();
        summary.push_str(&md::table(&["Date", "Title", "Summary"], &rows));
        summary.push('\n');
    }
    if series.contains(&Series::Stacks) {
        summary.push_str(&format!(
            "**CDC Stacks** (guidelines, reports, and archived pages): search here, newest first: {stacks_search_url}\n"
        ));
    }
    caveats.push(
        "HAN matches are by words in the title and CDC's description; read the notice for the \
         full recommendations and check for later Health Updates on the same topic."
            .into(),
    );
    caveats.push(
        "CDC reorganized its website in 2024–2025; older links may redirect, and some pages were \
         changed or removed. Confirm that guidance is current before acting on it."
            .into(),
    );
    caveats.push(
        "Massachusetts DPH guidance and 105 CMR requirements may differ from or add to CDC \
         guidance; follow DPH for state requirements."
            .into(),
    );
    if sources.is_empty() {
        sources.push(Source {
            title: "CDC Stacks".into(),
            publisher: "Centers for Disease Control and Prevention".into(),
            url: Some(stacks_search_url.clone()),
            effective_date: None,
            retrieved: None,
            mode: SourceMode::Live,
        });
    }
    Ok(Envelope::new(
        summary,
        CdcGuidanceResult {
            topic,
            items,
            searched,
            stacks_search_url,
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

    #[tokio::test]
    async fn offline_reports_unreachable_sources_or_link_only() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let err = search_cdc_guidance(
            &ctx,
            CdcGuidanceArgs {
                topic: "measles".into(),
                ..CdcGuidanceArgs::default()
            },
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(err.contains("stacks.cdc.gov"), "{err}");
        // Stacks alone works offline (a link).
        let env = search_cdc_guidance(
            &ctx,
            CdcGuidanceArgs {
                topic: "lead poisoning".into(),
                series: Some(vec![Series::Stacks]),
                ..CdcGuidanceArgs::default()
            },
        )
        .await
        .unwrap();
        assert!(env.data.stacks_search_url.contains("lead%20poisoning"));
        let err = search_cdc_guidance(&ctx, CdcGuidanceArgs::default()).await.unwrap_err();
        assert!(err.to_string().contains("topic"));
    }
}
