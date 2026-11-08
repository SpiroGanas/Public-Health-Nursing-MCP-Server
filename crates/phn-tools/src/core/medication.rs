//! CORE-07 `lookup_medication_information`.
//!
//! Reference lookups in RxNorm (names and related concepts), openFDA (drug
//! labels, enforcement reports, drug shortages), and DailyMed (package
//! inserts). Live with a one-day cache; there is no offline snapshot.

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::fetch::fetch_with_fallback;
use phn_data::medication::{
    self, Endpoints, LabelSummary, RecallSummary, RxConcept, ShortageSummary, SplSummary,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context::Context;
use crate::md;

/// Environment variable for an optional openFDA API key.
pub const OPENFDA_KEY_ENV: &str = "PHN_OPENFDA_API_KEY";
const TTL: u64 = 86_400;

/// Arguments for `lookup_medication_information`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct MedicationArgs {
    /// Drug name (brand or generic), e.g. "amoxicillin", "EpiPen", "oseltamivir".
    pub query: String,
    /// Sections to include: rxnorm, label, recalls, shortages, dailymed
    /// (default all).
    pub include: Option<Vec<String>>,
    /// Results per section (default 3, max 10).
    pub limit: Option<u32>,
}

/// One section's status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SectionStatus {
    pub section: String,
    /// live, cache, no_results, or unavailable.
    pub status: String,
    pub total_matches: Option<u64>,
    pub data_date: Option<String>,
    pub note: Option<String>,
}

/// Result of `lookup_medication_information`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MedicationResult {
    pub query: String,
    /// Best RxNorm concept for the query.
    pub rxnorm: Option<RxConcept>,
    /// Ingredients and brand names related to it.
    pub rxnorm_related: Vec<RxConcept>,
    pub labels: Vec<LabelSummary>,
    pub recalls: Vec<RecallSummary>,
    pub shortages: Vec<ShortageSummary>,
    pub dailymed: Vec<SplSummary>,
    pub sections: Vec<SectionStatus>,
}

enum Got {
    Json(Value, SourceMode, String),
    NoResults,
    Unavailable(String),
}

async fn get_json(ctx: &Context, url: &str) -> Got {
    match fetch_with_fallback(&ctx.http, ctx.cache.as_ref(), url, TTL, None).await {
        Ok(f) => match serde_json::from_slice::<Value>(&f.body) {
            Ok(v) => Got::Json(v, f.mode, f.as_of),
            Err(e) => Got::Unavailable(format!("unexpected response ({e})")),
        },
        Err(e) => {
            let m = e.to_string();
            if m.contains("404") {
                Got::NoResults
            } else {
                // Never echo the URL (it may carry the API key).
                let short = if m.contains("offline") {
                    "offline mode is on".to_string()
                } else {
                    "the service could not be reached".to_string()
                };
                Got::Unavailable(short)
            }
        }
    }
}

/// Look up FDA labeling, RxNorm names, recalls, and shortages (reference only).
pub async fn lookup_medication_information(
    ctx: &Context,
    args: MedicationArgs,
) -> Result<Envelope<MedicationResult>> {
    let key = std::env::var(OPENFDA_KEY_ENV).ok();
    lookup_with(ctx, args, &Endpoints::default(), key.as_deref()).await
}

/// The lookup with explicit endpoints (tests point these at a local server).
pub async fn lookup_with(
    ctx: &Context,
    args: MedicationArgs,
    ep: &Endpoints,
    api_key: Option<&str>,
) -> Result<Envelope<MedicationResult>> {
    let q = args.query.trim().to_string();
    if q.is_empty() {
        return Err(PhnError::InvalidInput(
            "query is empty; give a drug name".into(),
        ));
    }
    if q.len() > 100 {
        return Err(PhnError::InvalidInput(
            "query is too long; give a drug name".into(),
        ));
    }
    let limit = args.limit.unwrap_or(3).clamp(1, 10);
    let all = ["rxnorm", "label", "recalls", "shortages", "dailymed"];
    let include: Vec<String> = match &args.include {
        Some(v) if !v.is_empty() => {
            let v: Vec<String> = v.iter().map(|s| s.trim().to_ascii_lowercase()).collect();
            if let Some(bad) = v.iter().find(|s| !all.contains(&s.as_str())) {
                return Err(PhnError::InvalidInput(format!(
                    "unknown section '{bad}'; use any of {}",
                    all.join(", ")
                )));
            }
            v
        }
        _ => all.iter().map(|s| s.to_string()).collect(),
    };
    let want = |s: &str| include.iter().any(|i| i == s);
    let mut sections = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    let mut status = |section: &str, got: &Got, total: Option<u64>, date: Option<String>| {
        let (st, note) = match got {
            Got::Json(_, mode, _) => (mode.label().to_string(), None),
            Got::NoResults => ("no_results".to_string(), None),
            Got::Unavailable(m) => ("unavailable".to_string(), Some(m.clone())),
        };
        sections.push(SectionStatus {
            section: section.into(),
            status: st,
            total_matches: total,
            data_date: date,
            note,
        });
    };
    let add_source = |sources: &mut Vec<Source>,
                      title: &str,
                      publisher: &str,
                      url: &str,
                      mode: SourceMode,
                      as_of: &str,
                      eff: Option<String>| {
        if !sources.iter().any(|s| s.title == title) {
            sources.push(Source {
                title: title.into(),
                publisher: publisher.into(),
                url: Some(url.into()),
                effective_date: eff,
                retrieved: Some(as_of.chars().take(10).collect()),
                mode,
            });
        }
    };

    // RxNorm.
    let mut rxnorm = None;
    let mut related = Vec::new();
    let mut search_term = q.clone();
    if want("rxnorm") {
        let got = get_json(ctx, &ep.rxnav_approximate(&q, 5)).await;
        if let Got::Json(v, mode, as_of) = &got {
            add_source(
                &mut sources,
                "RxNorm (RxNav API)",
                "National Library of Medicine",
                "https://lhncbc.nlm.nih.gov/RxNav/",
                *mode,
                as_of,
                None,
            );
            if let Some((rxcui, _)) = medication::parse_approximate(v).into_iter().next() {
                if let Got::Json(p, _, _) = get_json(ctx, &ep.rxnav_properties(&rxcui)).await {
                    rxnorm = medication::parse_properties(&p);
                }
                if let Got::Json(r, _, _) = get_json(ctx, &ep.rxnav_related(&rxcui)).await {
                    related = medication::parse_related(&r);
                    related.retain(|c| c.tty == "IN" || c.tty == "BN");
                    related.truncate(12);
                }
            }
        }
        status("rxnorm", &got, None, None);
        // Search openFDA by the RxNorm ingredient name when we have one.
        if let Some(c) = &rxnorm
            && c.tty == "IN"
        {
            search_term = c.name.clone();
        }
    }

    let mut labels = Vec::new();
    if want("label") {
        let got = get_json(ctx, &ep.openfda_label(&search_term, limit, api_key)).await;
        let mut total = None;
        let mut date = None;
        if let Got::Json(v, mode, as_of) = &got {
            labels = medication::parse_labels(v);
            total = Some(medication::openfda_total(v));
            date = medication::openfda_last_updated(v);
            add_source(
                &mut sources,
                "openFDA drug labeling (drug/label)",
                "U.S. Food and Drug Administration",
                "https://open.fda.gov/apis/drug/label/",
                *mode,
                as_of,
                date.clone(),
            );
        }
        status("label", &got, total, date);
    }
    let mut recalls = Vec::new();
    if want("recalls") {
        let got = get_json(ctx, &ep.openfda_recalls(&search_term, limit, api_key)).await;
        let mut total = None;
        let mut date = None;
        if let Got::Json(v, mode, as_of) = &got {
            recalls = medication::parse_recalls(v);
            total = Some(medication::openfda_total(v));
            date = medication::openfda_last_updated(v);
            add_source(
                &mut sources,
                "openFDA drug enforcement reports (recalls)",
                "U.S. Food and Drug Administration",
                "https://open.fda.gov/apis/drug/enforcement/",
                *mode,
                as_of,
                date.clone(),
            );
        }
        status("recalls", &got, total, date);
    }
    let mut shortages = Vec::new();
    if want("shortages") {
        let got = get_json(ctx, &ep.openfda_shortages(&search_term, limit, api_key)).await;
        let mut total = None;
        let mut date = None;
        if let Got::Json(v, mode, as_of) = &got {
            shortages = medication::parse_shortages(v);
            total = Some(medication::openfda_total(v));
            date = medication::openfda_last_updated(v);
            add_source(
                &mut sources,
                "openFDA drug shortages",
                "U.S. Food and Drug Administration",
                "https://open.fda.gov/apis/drug/drugshortages/",
                *mode,
                as_of,
                date.clone(),
            );
        }
        status("shortages", &got, total, date);
    }
    let mut dailymed = Vec::new();
    if want("dailymed") {
        let got = get_json(ctx, &ep.dailymed_spls(&q, limit)).await;
        let mut total = None;
        if let Got::Json(v, mode, as_of) = &got {
            dailymed = medication::parse_spls(v);
            total = Some(medication::dailymed_total(v));
            add_source(
                &mut sources,
                "DailyMed Structured Product Labeling",
                "National Library of Medicine",
                "https://dailymed.nlm.nih.gov/dailymed/",
                *mode,
                as_of,
                None,
            );
        }
        status("dailymed", &got, total, None);
    }

    // Summary.
    let mut summary = format!("**Medication reference: {q}** (reference only)\n\n");
    if let Some(c) = &rxnorm {
        summary.push_str(&format!(
            "RxNorm: **{}** (RxCUI {}, {})",
            c.name, c.rxcui, c.tty
        ));
        let brands: Vec<&str> = related
            .iter()
            .filter(|r| r.tty == "BN")
            .map(|r| r.name.as_str())
            .take(8)
            .collect();
        if !brands.is_empty() {
            summary.push_str(&format!("; brand names: {}", brands.join(", ")));
        }
        summary.push_str("\n\n");
    }
    for l in &labels {
        summary.push_str(&format!(
            "**Label: {}** ({}){} — effective {}, set ID {}, version {}\n",
            l.brand_names
                .first()
                .cloned()
                .unwrap_or_else(|| l.generic_names.join(", ")),
            l.generic_names.join(", "),
            l.manufacturer
                .as_ref()
                .map(|m| format!(", {m}"))
                .unwrap_or_default(),
            l.effective_date
                .clone()
                .unwrap_or_else(|| "not stated".into()),
            l.set_id.clone().unwrap_or_default(),
            l.version.clone().unwrap_or_default()
        ));
        if let Some(b) = &l.boxed_warning {
            summary.push_str(&format!("- ⚠ Boxed warning: {b}\n"));
        }
        if let Some(i) = &l.indications {
            summary.push_str(&format!("- Indications: {i}\n"));
        }
        if let Some(c) = &l.contraindications {
            summary.push_str(&format!("- Contraindications: {c}\n"));
        }
        if let Some(set) = &l.set_id {
            summary.push_str(&format!(
                "- Full label: {}\n",
                medication::dailymed_setid_url(set)
            ));
        }
        summary.push('\n');
    }
    let checked = |section: &str| {
        sections
            .iter()
            .any(|s| s.section == section && s.status != "unavailable")
    };
    if checked("recalls") {
        if recalls.is_empty() {
            summary.push_str(
                "**Recalls:** none found in openFDA enforcement reports for this name.\n\n",
            );
        } else {
            let rows: Vec<Vec<String>> = recalls
                .iter()
                .map(|r| {
                    vec![
                        r.recall_number.clone(),
                        r.classification.clone(),
                        r.status.clone(),
                        r.report_date.clone().unwrap_or_default(),
                        r.reason_for_recall.clone(),
                    ]
                })
                .collect();
            summary.push_str("**Recalls (newest first)**\n\n");
            summary.push_str(&md::table(
                &["Recall", "Class", "Status", "Reported", "Reason"],
                &rows,
            ));
            summary.push('\n');
        }
    }
    if checked("shortages") {
        if shortages.is_empty() {
            summary.push_str(
                "**Shortages:** none listed in openFDA's drug shortage data for this name.\n\n",
            );
        } else {
            let rows: Vec<Vec<String>> = shortages
                .iter()
                .map(|s| {
                    vec![
                        s.generic_name.clone(),
                        s.status.clone(),
                        s.presentation.clone().unwrap_or_default(),
                        s.update_date.clone().unwrap_or_default(),
                    ]
                })
                .collect();
            summary.push_str("**Shortages**\n\n");
            summary.push_str(&md::table(
                &["Drug", "Status", "Presentation", "Updated"],
                &rows,
            ));
            summary.push('\n');
        }
    }
    if !dailymed.is_empty() {
        summary.push_str("**DailyMed package inserts**\n");
        for s in &dailymed {
            summary.push_str(&format!(
                "- {} (version {}, published {}): {}\n",
                s.title,
                s.spl_version.map(|v| v.to_string()).unwrap_or_default(),
                s.published_date.clone().unwrap_or_default(),
                s.url
            ));
        }
    }
    let unavailable: Vec<&SectionStatus> = sections
        .iter()
        .filter(|s| s.status == "unavailable")
        .collect();
    if !unavailable.is_empty() {
        summary.push_str(&format!(
            "\nNot checked ({}): {}. Search dailymed.nlm.nih.gov or open.fda.gov directly.\n",
            unavailable
                .first()
                .and_then(|s| s.note.clone())
                .unwrap_or_default(),
            unavailable
                .iter()
                .map(|s| s.section.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let mut caveats = vec![
        "Reference only. openFDA states its data are unvalidated and must not be relied on for medical decisions; the FDA-approved label (package insert) is the authority. Check your agency's protocols and a pharmacist before acting.".to_string(),
        "Label excerpts are shortened; read the full label (DailyMed link) before use.".to_string(),
    ];
    if labels.is_empty()
        && want("label")
        && !sections
            .iter()
            .any(|s| s.section == "label" && s.status == "unavailable")
    {
        caveats.push("No openFDA label matched this name; vaccines and some biologics are often missing from openFDA labeling. Try DailyMed or lookup_vaccine_product.".into());
    }
    if sections.iter().all(|s| s.status == "unavailable") {
        caveats.push("No live medication source could be reached and there is no offline copy; nothing was checked.".into());
    }
    Ok(Envelope::new(
        summary,
        MedicationResult {
            query: q,
            rxnorm,
            rxnorm_related: related,
            labels,
            recalls,
            shortages,
            dailymed,
            sections,
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
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn fixture(path: &str) -> &'static str {
        if path.contains("approximateTerm") {
            include_str!("../../../phn-data/tests/fixtures/medication/rxnav_approximate.json")
        } else if path.contains("properties") {
            include_str!("../../../phn-data/tests/fixtures/medication/rxnav_properties.json")
        } else if path.contains("related") {
            include_str!("../../../phn-data/tests/fixtures/medication/rxnav_related.json")
        } else if path.contains("label.json") {
            include_str!("../../../phn-data/tests/fixtures/medication/openfda_label.json")
        } else if path.contains("enforcement") {
            include_str!("../../../phn-data/tests/fixtures/medication/openfda_enforcement.json")
        } else if path.contains("spls") {
            include_str!("../../../phn-data/tests/fixtures/medication/dailymed_spls.json")
        } else {
            ""
        }
    }

    /// A local server that answers each request from the recorded fixtures
    /// (shortages answer 404, as openFDA does when nothing matches).
    async fn serve() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let path = req.split_whitespace().nth(1).unwrap_or("").to_string();
                    let body = fixture(&path);
                    let resp = if body.is_empty() {
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_string()
                    } else {
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    };
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn looks_up_all_sections_from_recorded_responses() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = phn_core::Config::for_root(dir.path());
        config.offline = false;
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Arc::new(Context::new(config).unwrap());
        let base = serve().await;
        let ep = Endpoints {
            openfda: base.clone(),
            dailymed: base.clone(),
            rxnav: base,
        };
        let env = lookup_with(
            &ctx,
            MedicationArgs {
                query: "amoxicilin".into(),
                ..Default::default()
            },
            &ep,
            Some("secret-key"),
        )
        .await
        .unwrap();
        let d = &env.data;
        assert_eq!(d.rxnorm.as_ref().unwrap().name, "amoxicillin");
        assert_eq!(d.labels.len(), 1);
        assert_eq!(d.recalls[0].recall_number, "D-0151-2026");
        assert!(d.shortages.is_empty());
        assert_eq!(
            d.sections
                .iter()
                .find(|s| s.section == "shortages")
                .unwrap()
                .status,
            "no_results"
        );
        assert!(!d.dailymed.is_empty());
        let md = env.to_markdown();
        assert!(md.contains("set ID 00b86913-50c8-443f-8467-f4f499d358af"));
        assert!(!md.contains("secret-key"), "API key must never be returned");
        assert!(env.notice.is_some());
    }

    #[tokio::test]
    async fn offline_reports_what_was_not_checked() {
        let dir = tempfile::tempdir().unwrap();
        let config = phn_core::Config::for_root(dir.path());
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(config).unwrap();
        let env = lookup_medication_information(
            &ctx,
            MedicationArgs {
                query: "oseltamivir".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(env.data.sections.iter().all(|s| s.status == "unavailable"));
        assert!(
            env.caveats
                .iter()
                .any(|c| c.contains("nothing was checked"))
        );
        let bad = lookup_medication_information(
            &ctx,
            MedicationArgs {
                query: " ".into(),
                ..Default::default()
            },
        )
        .await;
        assert!(bad.is_err());
    }
}
