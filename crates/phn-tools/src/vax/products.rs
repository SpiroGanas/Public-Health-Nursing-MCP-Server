//! VAX-07 `lookup_vaccine_product`.

use phn_core::text;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::fetch::fetch_with_fallback;
use phn_data::medication::{self, SplSummary};
use phn_data::vaccine_codes::{self, Cvx, VaccineCodes};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;
use crate::vax::content::{self, PRODUCT_INFO, ProductInfo, VisEdition};

/// Arguments for `lookup_vaccine_product`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ProductArgs {
    /// Product or vaccine name ("Shingrix", "MMR", "pneumococcal 20"), CVX code
    /// ("03" or "CVX 208"), MVX code ("PMC"), or NDC ("00006-4681-00" or 10/11 digits).
    pub query: String,
    /// Include inactive and historical codes (default false).
    pub include_inactive: bool,
    /// Most matches to return (default 5, max 25).
    pub limit: Option<usize>,
    /// Also search DailyMed live for the current package insert (setid,
    /// version, and publication date). Default false.
    pub include_label: bool,
}

/// A trade name tied to a CVX code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TradeNameInfo {
    pub trade_name: String,
    pub manufacturer: String,
    pub mvx: String,
    pub status: String,
}

/// An NDC summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NdcInfo {
    pub ndc: String,
    pub level: String,
    pub proprietary_name: String,
    pub packaging: String,
    pub labeler: String,
    pub end_date: Option<String>,
}

/// One matched product or vaccine code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProductMatch {
    pub cvx: String,
    pub short_description: String,
    pub full_name: String,
    pub status: String,
    pub matched_on: String,
    pub trade_names: Vec<TradeNameInfo>,
    /// Active NDCs (unit of sale and unit of use), up to 12.
    pub ndcs: Vec<NdcInfo>,
    pub ndc_total: usize,
    /// Licensed ages and storage from bundled content, when available.
    pub product_info: Vec<ProductInfo>,
    /// Current Vaccine Information Statement edition(s).
    pub vis: Vec<VisEdition>,
    /// DailyMed labels found (include_label).
    pub labels: Vec<SplSummary>,
}

/// Result of `lookup_vaccine_product`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProductLookup {
    pub query: String,
    pub matches: Vec<ProductMatch>,
    /// When the query was an MVX code: the manufacturer.
    pub manufacturer: Option<String>,
    /// Where the CVX table came from: live, cache, or snapshot.
    pub cvx_table_mode: String,
    pub vis_translations_url: String,
    pub beyond_use_guidance: Vec<String>,
}

fn vis_for(cvx: &Cvx, trade: &[TradeNameInfo]) -> Vec<VisEdition> {
    let mut hay = text::normalize(&format!("{} {}", cvx.short_description, cvx.full_name));
    for t in trade {
        hay.push(' ');
        hay.push_str(&text::normalize(&t.trade_name));
    }
    let mut keys: Vec<&str> = PRODUCT_INFO
        .vis_keywords
        .iter()
        .filter(|k| {
            k.keywords.iter().any(|w| {
                let w = text::normalize(w);
                !w.is_empty() && format!(" {hay} ").contains(&format!(" {w} "))
                    || hay.contains(&w) && w.len() > 4
            })
        })
        .map(|k| k.vaccine.as_str())
        .collect();
    // MMRV is more specific than MMR; influenza live vs inactivated handled by title.
    if keys.contains(&"mmrv") {
        keys.retain(|k| *k != "mmr" && *k != "varicella");
    }
    if keys.contains(&"tdap") || keys.contains(&"dtap") {
        keys.retain(|k| *k != "td");
    }
    let mut out: Vec<VisEdition> = PRODUCT_INFO
        .vis
        .editions
        .iter()
        .filter(|e| keys.contains(&e.vaccine.as_str()))
        .filter(|e| {
            e.vaccine != "influenza"
                || (hay.contains("live") == e.title.to_ascii_lowercase().contains("live"))
        })
        .cloned()
        .collect();
    out.dedup();
    out
}

fn build_match(
    codes: &VaccineCodes,
    cvx: &Cvx,
    matched_on: String,
    include_inactive: bool,
) -> ProductMatch {
    let trade: Vec<TradeNameInfo> = codes
        .trade_names_for_cvx(&cvx.cvx)
        .into_iter()
        .filter(|t| include_inactive || t.product_status.eq_ignore_ascii_case("active"))
        .map(|t| TradeNameInfo {
            trade_name: t.trade_name.clone(),
            manufacturer: t.manufacturer.clone(),
            mvx: t.mvx.clone(),
            status: t.product_status.clone(),
        })
        .collect();
    let all_ndc = codes.ndcs_for_cvx(&cvx.cvx);
    // NDCs with a future end date are still marketed.
    let today = chrono::Local::now().date_naive().to_string();
    let active: Vec<&&phn_data::vaccine_codes::VaccineNdc> = all_ndc
        .iter()
        .filter(|n| {
            include_inactive || n.end_date.is_empty() || n.end_date.as_str() > today.as_str()
        })
        .collect();
    let ndcs = active
        .iter()
        .take(12)
        .map(|n| NdcInfo {
            ndc: n.ndc.clone(),
            level: n.level.clone(),
            proprietary_name: n.proprietary_name.clone(),
            packaging: n.packaging.clone(),
            labeler: n.labeler.clone(),
            end_date: (!n.end_date.is_empty()).then(|| n.end_date.clone()),
        })
        .collect();
    let names: Vec<String> = trade
        .iter()
        .map(|t| text::normalize(&t.trade_name))
        .chain(all_ndc.iter().map(|n| text::normalize(&n.proprietary_name)))
        .collect();
    let product_info: Vec<ProductInfo> = PRODUCT_INFO
        .products
        .iter()
        .filter(|p| names.contains(&text::normalize(&p.trade_name)))
        .cloned()
        .collect();
    ProductMatch {
        vis: vis_for(cvx, &trade),
        cvx: cvx.cvx.clone(),
        short_description: cvx.short_description.clone(),
        full_name: cvx.full_name.clone(),
        status: cvx.status.clone(),
        matched_on,
        trade_names: trade,
        ndc_total: active.len(),
        ndcs,
        product_info,
        labels: Vec::new(),
    }
}

fn looks_like_ndc(q: &str) -> bool {
    let digits = q.chars().filter(char::is_ascii_digit).count();
    q.chars()
        .all(|c| c.is_ascii_digit() || c == '-' || c == ' ')
        && (10..=11).contains(&digits)
}

/// Product reference by name, trade name, CVX, MVX, or NDC.
pub async fn lookup_vaccine_product(
    ctx: &Context,
    args: ProductArgs,
) -> Result<Envelope<ProductLookup>> {
    let q = args.query.trim().to_string();
    if q.is_empty() {
        return Err(PhnError::InvalidInput(
            "query is empty; give a vaccine or product name, CVX, MVX, or NDC".into(),
        ));
    }
    let limit = args.limit.unwrap_or(5).clamp(1, 25);
    let mut codes = VaccineCodes::load(&ctx.snapshots)?;
    let mut sources: Vec<Source> = Vec::new();
    let mut caveats: Vec<String> = Vec::new();
    let mut cvx_mode = "snapshot".to_string();
    match fetch_with_fallback(
        &ctx.http,
        ctx.cache.as_ref(),
        vaccine_codes::CVX_LIVE_URL,
        7 * 86_400,
        None,
    )
    .await
    {
        Ok(f) => {
            let parsed = vaccine_codes::parse_cvx_flat(&String::from_utf8_lossy(&f.body));
            if parsed.len() > 100 {
                cvx_mode = f.mode.label().to_string();
                codes = codes.with_cvx(parsed);
                sources.push(vaccine_codes::live_cvx_source(f.mode, &f.as_of));
            }
        }
        Err(e) => {
            tracing::debug!(error = %e, "live CVX unavailable; using snapshot");
        }
    }
    sources.push(codes.source.clone());

    let mut manufacturer = None;
    let mut matches: Vec<ProductMatch> = Vec::new();
    let upper = q.to_ascii_uppercase();
    if looks_like_ndc(&q) {
        let hits = codes.ndc_lookup(&q);
        let mut seen = Vec::new();
        for h in hits {
            if seen.contains(&h.cvx) {
                continue;
            }
            seen.push(h.cvx.clone());
            if let Some(c) = codes.cvx_code(&h.cvx) {
                let mut m = build_match(
                    &codes,
                    c,
                    format!("NDC {} ({})", h.ndc, h.proprietary_name),
                    args.include_inactive,
                );
                // Always show the NDC that was asked about, even if it has ended.
                if !m.ndcs.iter().any(|n| n.ndc == h.ndc) {
                    m.ndcs.insert(
                        0,
                        NdcInfo {
                            ndc: h.ndc.clone(),
                            level: h.level.clone(),
                            proprietary_name: h.proprietary_name.clone(),
                            packaging: h.packaging.clone(),
                            labeler: h.labeler.clone(),
                            end_date: (!h.end_date.is_empty()).then(|| h.end_date.clone()),
                        },
                    );
                }
                matches.push(m);
            }
        }
        if matches.is_empty() {
            return Err(PhnError::NotFound(format!(
                "NDC {q} is not in CDC's vaccine NDC tables (snapshot). Check the digits, or search by product name."
            )));
        }
    } else if let Some(c) = vaccine_codes::normalize_cvx(&q).and_then(|n| codes.cvx_code(&n)) {
        matches.push(build_match(
            &codes,
            c,
            format!("CVX {}", c.cvx),
            args.include_inactive,
        ));
    } else if upper.len() <= 4
        && upper.chars().all(|c| c.is_ascii_alphabetic())
        && let Some(m) = codes.mvx_code(&upper)
    {
        manufacturer = Some(format!("{} (MVX {}, {})", m.manufacturer, m.mvx, m.status));
        let mut cvxs: Vec<String> = codes
            .trade_names
            .iter()
            .filter(|t| t.mvx.eq_ignore_ascii_case(&m.mvx))
            .filter(|t| args.include_inactive || t.product_status.eq_ignore_ascii_case("active"))
            .map(|t| t.cvx.clone())
            .collect();
        cvxs.dedup();
        for cv in cvxs.iter().take(limit) {
            if let Some(c) = codes.cvx_code(cv) {
                matches.push(build_match(
                    &codes,
                    c,
                    format!("MVX {}", m.mvx),
                    args.include_inactive,
                ));
            }
        }
    } else {
        // Fuzzy search over CVX descriptions, trade names, and product content.
        #[derive(Clone)]
        struct Cand {
            cvx: String,
            keys: Vec<String>,
        }
        let mut cands: Vec<Cand> = codes
            .cvx
            .iter()
            .filter(|c| args.include_inactive || c.status.eq_ignore_ascii_case("active"))
            .filter(|c| !c.non_vaccine.eq_ignore_ascii_case("true") || args.include_inactive)
            .map(|c| Cand {
                cvx: c.cvx.clone(),
                keys: vec![c.short_description.clone(), c.full_name.clone()],
            })
            .collect();
        for t in &codes.trade_names {
            if !args.include_inactive && !t.product_status.eq_ignore_ascii_case("active") {
                continue;
            }
            if let Some(c) = cands.iter_mut().find(|c| c.cvx == t.cvx) {
                c.keys.push(t.trade_name.clone());
            }
        }
        let ranked = text::rank(&q, &cands, |c| c.keys.clone(), 0.80);
        let top = ranked.first().map(|r| r.score).unwrap_or(0.0);
        for r in ranked
            .into_iter()
            .filter(|r| r.score >= top - 0.08)
            .take(limit)
        {
            if let Some(c) = codes.cvx_code(&cands[r.index].cvx) {
                matches.push(build_match(
                    &codes,
                    c,
                    r.matched_on.clone(),
                    args.include_inactive,
                ));
            }
        }
        if matches.is_empty() {
            return Err(PhnError::NotFound(format!(
                "no vaccine product matches '{q}'. Try a trade name (e.g. Shingrix), a vaccine (e.g. MMR), a CVX code, or an NDC; set include_inactive for discontinued products."
            )));
        }
    }

    // Optional live DailyMed label search.
    if args.include_label {
        let endpoints = medication::Endpoints::default();
        let mut label_mode: Option<SourceMode> = None;
        for m in matches.iter_mut().take(3) {
            let term = m
                .trade_names
                .first()
                .map(|t| t.trade_name.clone())
                .unwrap_or_else(|| m.short_description.clone());
            let url = endpoints.dailymed_spls(&term, 3);
            match fetch_with_fallback(&ctx.http, ctx.cache.as_ref(), &url, 86_400, None).await {
                Ok(f) => {
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&f.body) {
                        m.labels = medication::parse_spls(&v);
                        label_mode = Some(f.mode);
                    }
                }
                Err(e) => caveats.push(format!("DailyMed could not be reached for {term} ({e}); open dailymed.nlm.nih.gov to find the current package insert.")),
            }
        }
        if let Some(mode) = label_mode {
            sources.push(Source {
                title: "DailyMed Structured Product Labeling (SPL) search".into(),
                publisher: "National Library of Medicine".into(),
                url: Some("https://dailymed.nlm.nih.gov/dailymed/".into()),
                effective_date: None,
                retrieved: Some(chrono::Utc::now().date_naive().to_string()),
                mode,
            });
        }
    }

    let mut summary = format!("**Vaccine product lookup: {q}**\n\n");
    if let Some(m) = &manufacturer {
        summary.push_str(&format!("Manufacturer: {m}\n\n"));
    }
    for m in &matches {
        summary.push_str(&format!(
            "### CVX {} — {} ({})\n{}\n\n",
            m.cvx, m.short_description, m.status, m.full_name
        ));
        if !m.trade_names.is_empty() {
            summary.push_str(&format!(
                "- Trade names: {}\n",
                m.trade_names
                    .iter()
                    .map(|t| format!("{} ({}, MVX {})", t.trade_name, t.manufacturer, t.mvx))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        for p in &m.product_info {
            summary.push_str(&format!(
                "- {}: licensed/minimum ages — {}; storage — {} (source: {})\n",
                p.trade_name,
                p.licensed_ages,
                PRODUCT_INFO
                    .storage_classes
                    .get(&p.storage)
                    .cloned()
                    .unwrap_or_else(|| p.storage.clone()),
                p.ages_source
            ));
        }
        if !m.vis.is_empty() {
            summary.push_str(&format!(
                "- Current VIS: {}\n",
                m.vis
                    .iter()
                    .map(|v| format!(
                        "{} ({}{})",
                        v.title,
                        v.date.clone().unwrap_or_default(),
                        if v.interim { ", interim" } else { "" }
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if !m.ndcs.is_empty() {
            summary.push('\n');
            let rows: Vec<Vec<String>> = m
                .ndcs
                .iter()
                .map(|n| {
                    vec![
                        n.ndc.clone(),
                        n.level.clone(),
                        n.proprietary_name.clone(),
                        n.packaging.clone(),
                        n.end_date.clone().unwrap_or_default(),
                    ]
                })
                .collect();
            summary.push_str(&md::table(
                &["NDC", "Unit", "Product", "Packaging", "End date"],
                &rows,
            ));
            if m.ndc_total > m.ndcs.len() {
                summary.push_str(&format!("…{} NDCs in all.\n", m.ndc_total));
            }
        }
        for l in &m.labels {
            summary.push_str(&format!(
                "- DailyMed: {} (version {}, published {}) {}\n",
                l.title,
                l.spl_version.map(|v| v.to_string()).unwrap_or_default(),
                l.published_date.clone().unwrap_or_default(),
                l.url
            ));
        }
        summary.push('\n');
    }
    summary.push_str(&format!(
        "Translations of VISs: {} — {}\n",
        PRODUCT_INFO.vis.translations_url, PRODUCT_INFO.vis.translation_note
    ));
    let bud = &PRODUCT_INFO.beyond_use;
    let mut bud_lines = vec![bud.definition.clone()];
    bud_lines.extend(bud.calculation.iter().cloned());
    bud_lines.push(bud.note.clone());
    caveats.push(
        "Licensed ages, storage, and beyond-use dates come from the package insert, which is the \
         authority; products without bundled details point you to it (use include_label or \
         lookup_medication_information)."
            .into(),
    );
    caveats.push(format!("CVX table source: {cvx_mode}. MVX, trade names, and NDCs come from the bundled CDC snapshot."));
    caveats.extend(content::review_caveats(&[content::PRODUCTS]));
    sources.push(content::content_source(content::PRODUCTS)?);
    Ok(Envelope::new(
        summary,
        ProductLookup {
            query: q,
            matches,
            manufacturer,
            cvx_table_mode: cvx_mode,
            vis_translations_url: PRODUCT_INFO.vis.translations_url.clone(),
            beyond_use_guidance: bud_lines,
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
    fn ndc_shapes_are_recognized() {
        assert!(looks_like_ndc("00006-4681-00"));
        assert!(looks_like_ndc("0006468100"));
        assert!(looks_like_ndc("58160 0823 11"));
        assert!(!looks_like_ndc("187"));
        assert!(!looks_like_ndc("CVX 03"));
        assert!(!looks_like_ndc("123-456"));
    }

    #[test]
    fn vis_editions_follow_the_product() {
        let codes = VaccineCodes::load(&phn_data::snapshots::SnapshotStore::bundled()).unwrap();
        let vis = |cvx: &str| {
            let c = codes.cvx_code(cvx).unwrap();
            let m = build_match(&codes, c, String::new(), false);
            m.vis.into_iter().map(|v| v.vaccine).collect::<Vec<_>>()
        };
        assert_eq!(vis("03"), vec!["mmr".to_string()]);
        // MMRV gets its own VIS, not the MMR and varicella ones.
        let mmrv = vis("94");
        assert!(mmrv.contains(&"mmrv".to_string()), "{mmrv:?}");
        assert!(!mmrv.contains(&"mmr".to_string()));
        assert_eq!(vis("187"), vec!["zoster".to_string()]);
    }
}
