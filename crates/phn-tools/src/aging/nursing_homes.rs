//! AGE-06 `nursing_home_quality_lookup`: CMS Care Compare data for
//! Massachusetts nursing homes (star ratings, staffing, inspections and
//! infection control citations, penalties, and resident and staff
//! vaccination), live from the CMS Provider Data Catalog with the bundled
//! `cms-ma-nursing-homes` snapshot as fallback.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::census::GeoLevel;
use phn_data::cms_provider::{self, Averages, InfectionCitation, NursingHome, NursingHomeData};
use phn_data::geography::MaGeographies;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::aging::content::{
    self as c10, CitedOut, CitedText, Confidence, NURSING_HOMES, SourceRef, cite_all,
};
use crate::context::Context;
use crate::md;

#[derive(Debug, Clone, Deserialize)]
struct ScopeSeverity {
    confidence: Confidence,
    text: String,
    harm_letters: Vec<String>,
    immediate_jeopardy_letters: Vec<String>,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Tag {
    tag: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GuideContent {
    #[allow(dead_code)]
    meta: ContentMeta,
    sources: BTreeMap<String, SourceRef>,
    notes: Vec<CitedText>,
    scope_severity: ScopeSeverity,
    infection_control_tags: Vec<Tag>,
    infection_control_tags_source: String,
    contacts: Vec<CitedText>,
}

static CONTENT: LazyLock<GuideContent> =
    LazyLock::new(|| c10::load(NURSING_HOMES).expect("nursing home guide parses"));

/// How to order the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    /// Alphabetical (default).
    #[default]
    Name,
    /// Lowest overall star rating first.
    OverallRating,
    /// Lowest staffing rating first.
    Staffing,
    /// Most infection control citations first.
    InfectionControl,
    /// Lowest staff flu vaccination first.
    StaffFluVaccination,
}

/// Arguments for `nursing_home_quality_lookup`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct NursingHomeArgs {
    /// Nursing home name (partial names and typos are matched).
    pub name: Option<String>,
    /// CMS Certification Number (6 characters, e.g. "225002").
    pub ccn: Option<String>,
    /// City or town (default: the configured town when nothing else is given).
    pub town: Option<String>,
    /// County, e.g. "Worcester".
    pub county: Option<String>,
    /// Statewide summary instead of a list.
    pub statewide: bool,
    pub sort_by: Option<SortBy>,
    /// Most facilities to list (default 25).
    pub limit: Option<usize>,
    /// Include infection control citations (default: when 5 or fewer facilities).
    pub include_citations: Option<bool>,
}

/// A facility with its infection control citations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FacilityOut {
    #[serde(flatten)]
    pub facility: NursingHome,
    /// Infection control citations in the last three years (newest first).
    pub infection_control_citations: Vec<InfectionCitation>,
    /// Citations at harm level (G-L) in the infection control category.
    pub infection_control_harm_citations: usize,
    /// Plain-language flags (special focus, abuse icon, low ratings, ...).
    pub flags: Vec<String>,
}

/// Statewide counts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StatewideSummary {
    pub facilities: usize,
    /// Facilities by overall star rating ("1" to "5", "not rated").
    pub by_overall_rating: BTreeMap<String, usize>,
    pub special_focus: usize,
    pub abuse_icon: usize,
    pub with_infection_control_citations: usize,
    pub median_staff_flu_pct: Option<f64>,
    pub median_staff_covid_up_to_date_pct: Option<f64>,
    pub median_resident_covid_up_to_date_pct: Option<f64>,
}

/// Result of `nursing_home_quality_lookup`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NursingHomeResult {
    /// What was searched.
    pub query: String,
    pub matched: usize,
    pub facilities: Vec<FacilityOut>,
    pub statewide: Option<StatewideSummary>,
    pub averages: Vec<Averages>,
    /// Date of the CMS data (processing date) or snapshot.
    pub data_as_of: String,
    pub mode: SourceMode,
    pub how_to_read: Vec<CitedOut>,
    pub scope_severity_key: String,
    pub infection_control_tags: BTreeMap<String, String>,
    pub contacts: Vec<CitedOut>,
    pub care_compare_url: String,
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

fn stars(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.0}★")).unwrap_or_else(|| "—".into())
}

fn pct(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.0}%")).unwrap_or_else(|| "—".into())
}

/// The city or town a facility is in. CMS lists the postal city, which can be
/// a village or neighborhood ("Hyannis", "Dorchester") or a variant spelling
/// ("Middleboro"); when it is not one of the 351 municipalities, the main town
/// of the facility's ZIP Code Tabulation Area is used.
fn facility_town(
    ctx: &Context,
    geos: Option<&MaGeographies>,
    f: &NursingHome,
) -> Option<String> {
    if let Some(m) = ctx.ma.town(&f.city) {
        return Some(m.name.clone());
    }
    let zip: String = f.zip.trim().chars().take(5).collect();
    geos?
        .get(&zip)
        .filter(|g| g.level == GeoLevel::Zcta)
        .and_then(|g| g.town.clone())
}

async fn load(ctx: &Context) -> Result<(NursingHomeData, Vec<String>)> {
    let mut caveats = Vec::new();
    if !ctx.http.is_offline() {
        match cms_provider::load_live(&ctx.http, ctx.cache.as_ref()).await {
            Ok(d) => return Ok((d, caveats)),
            Err(e) => caveats.push(format!(
                "The live CMS Provider Data Catalog could not be used ({e}); the bundled snapshot is shown."
            )),
        }
    }
    let d = cms_provider::load_snapshot(&ctx.snapshots)?;
    Ok((d, caveats))
}

/// Look up Massachusetts nursing home quality data.
pub async fn nursing_home_quality_lookup(
    ctx: &Context,
    args: NursingHomeArgs,
) -> Result<Envelope<NursingHomeResult>> {
    let c = &*CONTENT;
    let any_filter =
        args.name.is_some() || args.ccn.is_some() || args.town.is_some() || args.county.is_some();
    let town = if !any_filter && !args.statewide {
        ctx.settings().jurisdiction.town
    } else {
        args.town.clone()
    };
    if !any_filter && !args.statewide && town.is_none() {
        return Err(PhnError::InvalidInput(
            "give a nursing home name, CMS Certification Number, town, or county, or set statewide (or configure your town)".into(),
        ));
    }
    let town = match town {
        Some(t) => Some(c10::resolve_town(ctx, &t)?.name.clone()),
        None => None,
    };
    let county = match args.county.as_deref() {
        Some(cn) => {
            let n = phn_core::text::normalize(cn);
            let n = n.strip_suffix(" county").unwrap_or(&n).to_string();
            let found = ctx
                .ma
                .municipalities
                .iter()
                .map(|m| m.county.clone())
                .find(|c| phn_core::text::normalize(c) == n)
                .ok_or_else(|| {
                    PhnError::NotFound(format!("'{cn}' is not a Massachusetts county"))
                })?;
            Some(found)
        }
        None => None,
    };
    let (data, mut caveats) = load(ctx).await?;
    let mut matches: Vec<&NursingHome> = data.facilities.iter().collect();
    let mut query = Vec::new();
    if let Some(ccn) = args.ccn.as_deref() {
        let ccn = ccn.trim();
        matches.retain(|f| f.ccn.eq_ignore_ascii_case(ccn));
        query.push(format!("CCN {ccn}"));
    }
    if let Some(t) = &town {
        let tu = phn_core::text::normalize(t);
        let geos = MaGeographies::shared(&ctx.snapshots).ok();
        matches.retain(|f| {
            facility_town(ctx, geos, f).is_some_and(|ft| phn_core::text::normalize(&ft) == tu)
        });
        let by_zip = matches
            .iter()
            .filter(|f| phn_core::text::normalize(&f.city) != tu)
            .count();
        if by_zip > 0 {
            caveats.push(format!(
                "{by_zip} of the facilities matched are listed by CMS under a village, neighborhood, or variant name (shown in parentheses) and were matched to {t} by ZIP code. A ZIP code can cross a town line; check the address."
            ));
        }
        query.push(format!("town {t}"));
    }
    if let Some(cn) = &county {
        matches.retain(|f| f.county.eq_ignore_ascii_case(cn));
        query.push(format!("{cn} County"));
    }
    if let Some(name) = args.name.as_deref().filter(|n| !n.trim().is_empty()) {
        let q = phn_core::text::normalize(name);
        let mut scored: Vec<(&NursingHome, f64)> = matches
            .iter()
            .map(|f| {
                let n = phn_core::text::normalize(&f.name);
                let sc = if n.contains(&q) {
                    1.0
                } else {
                    phn_core::text::similarity(&q, &n)
                };
                (*f, sc)
            })
            .filter(|(_, s)| *s >= 0.6)
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        matches = scored.into_iter().map(|(f, _)| f).collect();
        query.push(format!("name \"{name}\""));
    }
    if args.statewide && query.is_empty() {
        query.push("Massachusetts (statewide)".into());
    }
    if matches.is_empty() {
        return Err(PhnError::NotFound(format!(
            "no Massachusetts nursing home matched {} in CMS data ({} facilities, data as of {}). Check the spelling, or search by town or county.",
            query.join(", "),
            data.facilities.len(),
            data.as_of
        )));
    }
    let sort = args.sort_by.unwrap_or_default();
    if args.name.is_none() {
        match sort {
            SortBy::Name => matches.sort_by(|a, b| a.name.cmp(&b.name)),
            SortBy::OverallRating => matches.sort_by(|a, b| {
                a.overall_rating
                    .unwrap_or(9.0)
                    .total_cmp(&b.overall_rating.unwrap_or(9.0))
            }),
            SortBy::Staffing => matches.sort_by(|a, b| {
                a.staffing_rating
                    .unwrap_or(9.0)
                    .total_cmp(&b.staffing_rating.unwrap_or(9.0))
            }),
            SortBy::InfectionControl => {
                let n = |f: &NursingHome| data.citations.iter().filter(|c| c.ccn == f.ccn).count();
                matches.sort_by_key(|f| std::cmp::Reverse(n(f)));
            }
            SortBy::StaffFluVaccination => matches.sort_by(|a, b| {
                a.staff_flu_pct
                    .unwrap_or(999.0)
                    .total_cmp(&b.staff_flu_pct.unwrap_or(999.0))
            }),
        }
    }
    let matched = matches.len();
    let limit = args.limit.unwrap_or(25).clamp(1, 400);
    let shown: Vec<&NursingHome> = if args.statewide && args.name.is_none() && args.ccn.is_none() {
        matches.iter().take(limit.min(10)).copied().collect()
    } else {
        matches.iter().take(limit).copied().collect()
    };
    let with_citations = args.include_citations.unwrap_or(shown.len() <= 5);
    let harm: Vec<&str> = c
        .scope_severity
        .harm_letters
        .iter()
        .map(String::as_str)
        .collect();
    let facilities: Vec<FacilityOut> = shown
        .iter()
        .map(|f| {
            let cits: Vec<InfectionCitation> = data
                .citations
                .iter()
                .filter(|x| x.ccn == f.ccn)
                .cloned()
                .collect();
            let harm_n = cits
                .iter()
                .filter(|x| harm.contains(&x.scope_severity.as_str()))
                .count();
            let mut flags = Vec::new();
            if !f.special_focus_status.is_empty() {
                flags.push(format!("Special Focus status: {}", f.special_focus_status));
            }
            if f.abuse_icon {
                flags.push("CMS abuse icon (cited for abuse or neglect)".into());
            }
            if f.overall_rating.is_some_and(|r| r <= 1.0) {
                flags.push("1-star overall rating".into());
            }
            if harm_n > 0 {
                flags.push(format!(
                    "{harm_n} infection control citation(s) at harm level or higher"
                ));
            }
            FacilityOut {
                facility: (*f).clone(),
                infection_control_citations: if with_citations {
                    cits.clone()
                } else {
                    Vec::new()
                },
                infection_control_harm_citations: harm_n,
                flags,
            }
        })
        .collect();
    let statewide = args.statewide.then(|| {
        let all = &data.facilities;
        let mut by = BTreeMap::new();
        for f in all {
            let k = f
                .overall_rating
                .map(|r| format!("{r:.0}"))
                .unwrap_or_else(|| "not rated".into());
            *by.entry(k).or_insert(0) += 1;
        }
        StatewideSummary {
            facilities: all.len(),
            by_overall_rating: by,
            special_focus: all
                .iter()
                .filter(|f| !f.special_focus_status.is_empty())
                .count(),
            abuse_icon: all.iter().filter(|f| f.abuse_icon).count(),
            with_infection_control_citations: all
                .iter()
                .filter(|f| data.citations.iter().any(|x| x.ccn == f.ccn))
                .count(),
            median_staff_flu_pct: median(all.iter().filter_map(|f| f.staff_flu_pct).collect()),
            median_staff_covid_up_to_date_pct: median(
                all.iter()
                    .filter_map(|f| f.staff_covid_up_to_date_pct)
                    .collect(),
            ),
            median_resident_covid_up_to_date_pct: median(
                all.iter()
                    .filter_map(|f| f.resident_covid_up_to_date_pct)
                    .collect(),
            ),
        }
    });

    // ---- markdown
    let mut s = format!(
        "**Nursing home quality (CMS Care Compare) — {}**\n\n{} facility(ies) matched{}. Data as of {} ({}).\n\n",
        query.join(", "),
        matched,
        if matched > facilities.len() {
            format!("; showing {}", facilities.len())
        } else {
            String::new()
        },
        data.as_of,
        data.mode.label()
    );
    if let Some(sw) = &statewide {
        s.push_str(&format!(
            "Statewide: {} nursing homes; overall rating counts {}; {} Special Focus; {} with the abuse icon; {} with infection control citations in 3 years. Median staff flu vaccination {}, staff COVID-19 up to date {}, residents COVID-19 up to date {}.\n\n",
            sw.facilities,
            sw.by_overall_rating.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join(", "),
            sw.special_focus,
            sw.abuse_icon,
            sw.with_infection_control_citations,
            pct(sw.median_staff_flu_pct),
            pct(sw.median_staff_covid_up_to_date_pct),
            pct(sw.median_resident_covid_up_to_date_pct)
        ));
    }
    let rows: Vec<Vec<String>> = facilities
        .iter()
        .map(|o| {
            let f = &o.facility;
            vec![
                format!("{} ({})", f.name, f.city),
                format!(
                    "{} / {} / {} / {}",
                    stars(f.overall_rating),
                    stars(f.health_inspection_rating),
                    stars(f.staffing_rating),
                    stars(f.quality_measure_rating)
                ),
                f.total_nurse_hours_per_resident_day
                    .map(|v| format!("{v:.2}"))
                    .unwrap_or_else(|| "—".into()),
                format!(
                    "{} ({} harm)",
                    data.citations.iter().filter(|x| x.ccn == f.ccn).count(),
                    o.infection_control_harm_citations
                ),
                format!(
                    "{} / {}",
                    pct(f.staff_flu_pct),
                    pct(f.staff_covid_up_to_date_pct)
                ),
                format!(
                    "{} / {}",
                    pct(f.long_stay_flu_pct),
                    pct(f.resident_covid_up_to_date_pct)
                ),
                o.flags.join("; "),
            ]
        })
        .collect();
    s.push_str(&md::table(
        &[
            "Nursing home",
            "Overall / inspection / staffing / quality",
            "Nurse hrs per resident-day",
            "Infection control citations (3 yrs)",
            "Staff flu / COVID-19 up to date",
            "Residents flu (long stay) / COVID-19",
            "Flags",
        ],
        &rows,
    ));
    for a in &data.averages {
        s.push_str(&format!(
            "\n{} average: overall {:.1}, inspection {:.1}, staffing {:.1}, quality {:.1}; {:.2} nurse hours per resident-day; nursing staff turnover {:.0}%.",
            if a.level == "NATION" { "U.S." } else { "Massachusetts" },
            a.overall_rating.unwrap_or(f64::NAN),
            a.health_inspection_rating.unwrap_or(f64::NAN),
            a.staffing_rating.unwrap_or(f64::NAN),
            a.quality_measure_rating.unwrap_or(f64::NAN),
            a.total_nurse_hours_per_resident_day.unwrap_or(f64::NAN),
            a.nursing_staff_turnover_pct.unwrap_or(f64::NAN)
        ));
    }
    s.push('\n');
    if with_citations {
        for o in facilities
            .iter()
            .filter(|o| !o.infection_control_citations.is_empty())
        {
            s.push_str(&format!(
                "\n**Infection control citations — {}**\n",
                o.facility.name
            ));
            for x in o.infection_control_citations.iter().take(10) {
                s.push_str(&format!(
                    "- {} {} (scope/severity {}{}): {}\n",
                    x.survey_date,
                    x.tag,
                    x.scope_severity,
                    if x.infection_control_inspection {
                        ", infection control inspection"
                    } else if x.complaint {
                        ", complaint inspection"
                    } else {
                        ""
                    },
                    x.description
                ));
            }
        }
    }
    s.push_str(&format!(
        "\n_Scope/severity: {}_\n",
        c.scope_severity
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    ));
    s.push_str("\nContacts: ");
    s.push_str(
        &c.contacts
            .iter()
            .map(|x| x.text.clone())
            .collect::<Vec<_>>()
            .join(" "),
    );
    s.push('\n');

    let mut sources = Vec::new();
    match data.mode {
        SourceMode::Snapshot => {
            if let Some((e, _)) = ctx.snapshots.entry(cms_provider::SNAPSHOT_ID) {
                sources.push(e.to_source());
                caveats.extend(e.caveats.clone());
            }
        }
        mode => {
            for (id, title) in [
                (cms_provider::PROVIDER_INFO, "Provider Information"),
                (cms_provider::HEALTH_DEFICIENCIES, "Health Deficiencies"),
                (
                    cms_provider::SNF_QRP,
                    "Skilled Nursing Facility Quality Reporting Program - Provider Data",
                ),
                (cms_provider::MDS_QM, "MDS Quality Measures"),
                (cms_provider::STATE_AVERAGES, "State US Averages"),
            ] {
                sources.push(Source {
                    title: format!("{title} ({id})"),
                    publisher: "Centers for Medicare & Medicaid Services, Provider Data Catalog"
                        .into(),
                    url: Some(format!("https://data.cms.gov/provider-data/dataset/{id}")),
                    effective_date: Some(data.as_of.clone()),
                    retrieved: Some(chrono::Local::now().date_naive().to_string()),
                    mode,
                });
            }
        }
    }
    sources.push(c10::content_source(NURSING_HOMES)?);
    caveats.push("Star ratings and citations describe the facility, not any resident. Citations may be under dispute (informal dispute resolution) or already corrected; see each citation's status.".into());
    caveats.push(format!(
        "The scope/severity letter key is the standard CMS grid ({} confidence; {}).",
        c.scope_severity.confidence.label(),
        c10::citation(
            &c.sources,
            &c.scope_severity.source,
            &c.scope_severity.section
        )
    ));
    if facilities
        .iter()
        .any(|f| f.facility.staff_flu_pct.is_none())
    {
        caveats.push("Some facilities have no SNF QRP vaccination data (for example facilities without Medicare SNF beds, or too few staff or residents to report).".into());
    }
    caveats.extend(c10::review_caveats(&[NURSING_HOMES]));
    let ij: Vec<&str> = c
        .scope_severity
        .immediate_jeopardy_letters
        .iter()
        .map(String::as_str)
        .collect();
    if facilities.iter().any(|f| {
        f.infection_control_citations
            .iter()
            .any(|x| ij.contains(&x.scope_severity.as_str()))
    }) {
        caveats.push(
            "At least one infection control citation is at the immediate jeopardy level (J-L)."
                .into(),
        );
    }
    Ok(Envelope::new(
        s,
        NursingHomeResult {
            query: query.join(", "),
            matched,
            facilities,
            statewide,
            averages: data.averages.clone(),
            data_as_of: data.as_of.clone(),
            mode: data.mode,
            how_to_read: cite_all(&c.sources, &c.notes),
            scope_severity_key: c
                .scope_severity
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            infection_control_tags: c
                .infection_control_tags
                .iter()
                .map(|t| (t.tag.clone(), t.text.clone()))
                .collect(),
            contacts: cite_all(&c.sources, &c.contacts),
            care_compare_url: "https://www.medicare.gov/care-compare/".into(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        notes: vec![format!(
            "Public facility-level data only ({}).",
            c10::citation(&c.sources, &c.infection_control_tags_source, "")
        )],
        ..Privacy::none()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let mut config = phn_core::Config::for_root(dir.path());
        config.offline = true;
        let ctx = Context::new(config).unwrap();
        (dir, ctx)
    }

    #[tokio::test]
    async fn offline_lookup_uses_snapshot() {
        let (_d, ctx) = ctx();
        let r = nursing_home_quality_lookup(
            &ctx,
            NursingHomeArgs {
                town: Some("Worcester".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(r.data.mode, SourceMode::Snapshot);
        assert!(r.data.matched >= 5);
        assert!(
            r.data
                .facilities
                .iter()
                .all(|f| f.facility.city.eq_ignore_ascii_case("worcester"))
        );
        let one = &r.data.facilities[0].facility.ccn;
        let r = nursing_home_quality_lookup(
            &ctx,
            NursingHomeArgs {
                ccn: Some(one.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(r.data.matched, 1);
        let r = nursing_home_quality_lookup(
            &ctx,
            NursingHomeArgs {
                statewide: true,
                sort_by: Some(SortBy::InfectionControl),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let sw = r.data.statewide.unwrap();
        assert!(sw.facilities > 300);
        assert!(sw.with_infection_control_citations > 50);
        assert!(
            nursing_home_quality_lookup(
                &ctx,
                NursingHomeArgs {
                    name: Some("zzzz qqqq nonexistent".into()),
                    ..Default::default()
                }
            )
            .await
            .is_err()
        );
        assert!(
            nursing_home_quality_lookup(&ctx, NursingHomeArgs::default())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn town_search_includes_villages_and_variant_spellings() {
        let (_d, ctx) = ctx();
        let find = |town: &str| {
            nursing_home_quality_lookup(
                &ctx,
                NursingHomeArgs {
                    town: Some(town.into()),
                    limit: Some(100),
                    ..Default::default()
                },
            )
        };
        // CMS lists Barnstable's homes under Hyannis and Centerville.
        let r = find("Barnstable").await.unwrap();
        assert!(r.data.matched >= 2);
        assert!(r.caveats.iter().any(|c| c.contains("ZIP code")));
        // "MIDDLEBORO" in CMS is Middleborough.
        assert!(find("Middleborough").await.unwrap().data.matched >= 1);
        // Boston includes its neighborhoods (Dorchester, Mattapan, ...).
        let r = find("Boston").await.unwrap();
        assert!(
            r.data
                .facilities
                .iter()
                .any(|f| !f.facility.city.eq_ignore_ascii_case("boston"))
        );
        // A plain city-name match adds no ZIP caveat.
        let r = find("Worcester").await.unwrap();
        assert!(!r.caveats.iter().any(|c| c.contains("by ZIP code")));
    }
}
