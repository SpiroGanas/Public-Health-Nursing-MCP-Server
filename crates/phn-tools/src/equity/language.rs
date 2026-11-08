//! EQ-03 `language_access_profile`.

use std::collections::{BTreeMap, BTreeSet};

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::census::{self, AcsSnapshot, Estimate, GeoLevel};
use phn_data::geography::MaGeographies;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::acs;
use super::profile::ProfileLevel;
use crate::context::Context;
use crate::md;

/// Arguments for `language_access_profile`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct LanguageAccessArgs {
    /// A Massachusetts city or town, county, or census tract (11-digit GEOID).
    /// Default: your configured town.
    #[serde(default)]
    pub area: Option<String>,
    /// Level of `area` (default: detected).
    #[serde(default)]
    pub level: Option<ProfileLevel>,
    /// Size of the population your program serves, if it is not the whole area
    /// (counts are then scaled, assuming the same language mix as the area).
    #[serde(default)]
    pub eligible_population: Option<f64>,
}

/// Status of a language group against the threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdStatus {
    /// Even the low end of the 90% margin of error reaches the threshold.
    ClearlyMeets,
    /// The estimate reaches the threshold, but the low end of the margin does not.
    Meets,
    /// The estimate is below, but the high end of the margin reaches it.
    Possibly,
    /// Below the threshold.
    No,
}

impl ThresholdStatus {
    fn label(self) -> &'static str {
        match self {
            Self::ClearlyMeets => "meets (even at the low end of the margin of error)",
            Self::Meets => "meets (estimate; margin of error crosses the threshold)",
            Self::Possibly => "possibly (within the margin of error)",
            Self::No => "no",
        }
    }
}

/// One C16001 language group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LanguageGroupRow {
    pub group_id: String,
    pub language_group: String,
    /// Residents 5+ who speak the language at home.
    pub speakers: Option<Estimate>,
    /// Of those, speak English less than "very well" (LEP).
    pub lep: Estimate,
    /// LEP persons as % of the population 5+.
    pub lep_percent: Option<f64>,
    /// LEP persons in the eligible population (scaled when eligible_population is given).
    pub lep_in_eligible_population: f64,
    pub threshold_status: ThresholdStatus,
    /// The group reaches the 5% trigger but has fewer than 50 persons.
    pub small_group_notice: bool,
}

/// C16002 limited-English households.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HouseholdRow {
    pub language_group: String,
    pub households: Option<Estimate>,
    pub limited_english_households: Estimate,
}

/// B16001 detailed language.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DetailedRow {
    pub language: String,
    pub speakers: Estimate,
    pub lep: Estimate,
}

/// Result of `language_access_profile`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LanguageAccessResult {
    pub area: String,
    pub acs_period: String,
    pub acs_mode: SourceMode,
    pub population_5plus: Option<Estimate>,
    pub lep_total: Option<Estimate>,
    pub lep_percent: Option<f64>,
    /// Eligible population the threshold is measured against.
    pub eligible_population: f64,
    /// The threshold applied: the smaller of 5% of the eligible population and 1,000 persons.
    pub threshold_persons: f64,
    pub threshold_rule: String,
    pub groups: Vec<LanguageGroupRow>,
    pub households: Vec<HouseholdRow>,
    /// PUMAs (2020) used for detailed languages.
    pub pumas: Vec<String>,
    /// Detailed languages (B16001) for the PUMA(s) containing the area, by LEP count.
    pub puma_languages: Vec<DetailedRow>,
    /// The 15 languages most commonly spoken by Massachusetts residents with LEP (B16001).
    pub massachusetts_top_15: Vec<DetailedRow>,
}

#[derive(Debug, Clone, Deserialize)]
struct SafeHarbor {
    citation: String,
    confidence: String,
    threshold_percent: f64,
    threshold_count: f64,
    rule: String,
    small_group_rule: String,
    limits: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Regulation {
    citation: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TextEntry {
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Rules {
    meta: phn_content::ContentMeta,
    safe_harbor: SafeHarbor,
    regulations: Vec<Regulation>,
    data_limits: Vec<TextEntry>,
}

fn rules() -> Result<Rules> {
    let item = phn_content::ITEMS
        .iter()
        .find(|i| i.id == "language-access-rules")
        .ok_or_else(|| PhnError::Internal("language access content missing".into()))?;
    let v = phn_content::yaml_to_json(item.yaml).map_err(PhnError::Internal)?;
    serde_json::from_value(v).map_err(|e| PhnError::Internal(format!("language access content: {e}")))
}

fn sum_detail(rows: &[&census::DetailedLanguage]) -> Vec<DetailedRow> {
    let mut by: BTreeMap<String, (Vec<Estimate>, Vec<Estimate>)> = BTreeMap::new();
    for r in rows {
        let e = by.entry(r.language.clone()).or_default();
        e.0.push(r.speakers);
        e.1.push(r.lep);
    }
    let mut out: Vec<DetailedRow> = by
        .into_iter()
        .map(|(language, (s, l))| DetailedRow {
            language,
            speakers: census::sum(&s),
            lep: census::sum(&l),
        })
        .collect();
    out.sort_by(|a, b| b.lep.value.total_cmp(&a.lep.value));
    out
}

fn status(e: &Estimate, scale: f64, threshold: f64) -> ThresholdStatus {
    let v = e.value * scale;
    let m = e.moe.unwrap_or(0.0) * scale;
    if v - m >= threshold {
        ThresholdStatus::ClearlyMeets
    } else if v >= threshold {
        ThresholdStatus::Meets
    } else if v + m >= threshold {
        ThresholdStatus::Possibly
    } else {
        ThresholdStatus::No
    }
}

/// Limited English proficiency by language, with the HHS safe harbor threshold.
pub async fn language_access_profile(
    ctx: &Context,
    args: LanguageAccessArgs,
) -> Result<Envelope<LanguageAccessResult>> {
    let rules = rules()?;
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let query = match args.area.as_deref().filter(|a| !a.trim().is_empty()) {
        Some(a) => a.to_string(),
        None => ctx.settings().jurisdiction.town.ok_or_else(|| {
            PhnError::InvalidInput(
                "give an area (town, county, or tract), or set your town with \
                 configure_jurisdiction"
                    .into(),
            )
        })?,
    };
    let level = args.level.map(|l| match l {
        ProfileLevel::County => GeoLevel::County,
        ProfileLevel::Town => GeoLevel::Town,
        ProfileLevel::Tract => GeoLevel::Tract,
    });
    let geo = geos.resolve(&query, level)?;
    acs::check_level(geo)?;
    let fig = acs::figures(ctx, geo).await?;
    let pop5 = fig.values.get("pop5plus").copied();
    let lep_total = fig.values.get("lep_count").copied();
    let pop5_v = pop5.map(|p| p.value).unwrap_or(0.0);
    let eligible = match args.eligible_population {
        Some(e) if e.is_finite() && e > 0.0 => e,
        Some(_) => {
            return Err(PhnError::InvalidInput(
                "eligible_population must be a positive number".into(),
            ));
        }
        None => pop5_v,
    };
    let scale = if pop5_v > 0.0 { eligible / pop5_v } else { 0.0 };
    let sh = &rules.safe_harbor;
    let threshold = (sh.threshold_percent / 100.0 * eligible).min(sh.threshold_count);
    let five_pct = sh.threshold_percent / 100.0 * eligible;

    let mut groups = Vec::new();
    for (id, label, _, _) in census::LANGUAGE_GROUPS
        .iter()
        .chain(std::iter::once(&census::LANGUAGE_OTHER))
    {
        let Some(lep) = fig.values.get(&format!("lang_{id}_lep")).copied() else {
            continue;
        };
        let speakers = fig.values.get(&format!("lang_{id}_total")).copied();
        let scaled = lep.value * scale;
        let st = status(&lep, scale, threshold);
        groups.push(LanguageGroupRow {
            group_id: id.to_string(),
            language_group: label.to_string(),
            speakers,
            lep,
            lep_percent: (pop5_v > 0.0).then(|| lep.value / pop5_v * 100.0),
            lep_in_eligible_population: scaled.round(),
            threshold_status: st,
            small_group_notice: scaled >= five_pct && scaled < 50.0,
        });
    }
    groups.sort_by(|a, b| b.lep.value.total_cmp(&a.lep.value));
    let households: Vec<HouseholdRow> = census::HOUSEHOLD_LANGUAGE_GROUPS
        .iter()
        .filter_map(|(id, label, _, _)| {
            Some(HouseholdRow {
                language_group: label.to_string(),
                households: fig.values.get(&format!("hh_{id}_total")).copied(),
                limited_english_households: fig.values.get(&format!("hh_{id}_lep")).copied()?,
            })
        })
        .collect();

    // Detailed languages (B16001) from the snapshot.
    let snap = AcsSnapshot::shared(&ctx.snapshots)?;
    let pumas: BTreeSet<String> = match geo.level {
        GeoLevel::Tract => geo.puma.iter().cloned().collect(),
        GeoLevel::Town => geos
            .tracts_in_town(&geo.name)
            .iter()
            .filter_map(|t| t.puma.clone())
            .collect(),
        _ => BTreeSet::new(),
    };
    let puma_rows: Vec<&census::DetailedLanguage> = snap
        .detailed_languages
        .iter()
        .filter(|d| pumas.contains(&d.geoid))
        .collect();
    let mut puma_languages = sum_detail(&puma_rows);
    puma_languages.truncate(12);
    let ma_rows: Vec<&census::DetailedLanguage> = snap
        .detailed_languages
        .iter()
        .filter(|d| d.geoid == "25")
        .collect();
    let mut top15 = sum_detail(&ma_rows);
    top15.retain(|d| !d.language.starts_with("Other"));
    top15.truncate(15);

    // Summary.
    let qualifying: Vec<&LanguageGroupRow> = groups
        .iter()
        .filter(|g| matches!(g.threshold_status, ThresholdStatus::ClearlyMeets | ThresholdStatus::Meets))
        .collect();
    let possible: Vec<&LanguageGroupRow> = groups
        .iter()
        .filter(|g| g.threshold_status == ThresholdStatus::Possibly)
        .collect();
    let mut summary = format!(
        "**Language access profile: {}** (ACS {} 5-year, tables C16001, C16002, B16001)\n\n",
        geo.name,
        fig.period()
    );
    if let (Some(p), Some(l)) = (pop5, lep_total) {
        summary.push_str(&format!(
            "Residents age 5+: {} (±{}). Speak English less than \"very well\" (LEP): {} (±{}), {:.1}%.\n\n",
            md::num(p.value, 0),
            md::num(p.moe.unwrap_or(0.0), 0),
            md::num(l.value, 0),
            md::num(l.moe.unwrap_or(0.0), 0),
            if p.value > 0.0 { l.value / p.value * 100.0 } else { 0.0 }
        ));
    }
    summary.push_str(&format!(
        "**Safe harbor threshold** (HHS 2003 LEP guidance, draft content, see caveats): the smaller \
         of {}% of the eligible population ({}) and {} persons = **{}** LEP persons{}.\n\n",
        sh.threshold_percent,
        md::num(five_pct, 0),
        md::num(sh.threshold_count, 0),
        md::num(threshold, 0),
        if args.eligible_population.is_some() {
            format!(
                " (eligible population {} as given; group counts scaled assuming the same \
                 language mix as {})",
                md::num(eligible, 0),
                geo.name
            )
        } else {
            " (eligible population = residents age 5+)".into()
        }
    ));
    let rows: Vec<Vec<String>> = groups
        .iter()
        .map(|g| {
            vec![
                g.language_group.clone(),
                g.speakers
                    .map(|s| md::num(s.value, 0))
                    .unwrap_or("—".into()),
                format!(
                    "{} ±{}",
                    md::num(g.lep.value, 0),
                    md::num(g.lep.moe.unwrap_or(0.0), 0)
                ),
                g.lep_percent.map(|p| format!("{p:.1}%")).unwrap_or("—".into()),
                g.threshold_status.label().into(),
            ]
        })
        .collect();
    summary.push_str(&md::table(
        &["Language group (C16001)", "Speakers 5+", "LEP (±90% MOE)", "LEP % of 5+", "Meets threshold"],
        &rows,
    ));
    summary.push_str(&format!(
        "\n**Result:** {}{}\n\n",
        if qualifying.is_empty() {
            "no language group reaches the threshold".to_string()
        } else {
            format!(
                "translate vital documents for: {}",
                qualifying
                    .iter()
                    .map(|g| g.language_group.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        },
        if possible.is_empty() {
            String::new()
        } else {
            format!(
                ". Within the margin of error: {}",
                possible
                    .iter()
                    .map(|g| g.language_group.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        }
    ));
    if groups.iter().any(|g| g.small_group_notice) {
        summary.push_str(&format!("Small groups: {}\n\n", sh.small_group_rule));
    }
    if !households.is_empty() {
        let h: Vec<Vec<String>> = households
            .iter()
            .map(|r| {
                vec![
                    r.language_group.clone(),
                    format!(
                        "{} ±{}",
                        md::num(r.limited_english_households.value, 0),
                        md::num(r.limited_english_households.moe.unwrap_or(0.0), 0)
                    ),
                ]
            })
            .collect();
        summary.push_str("**Limited English speaking households (C16002)** (no one age 14+ speaks English only or very well)\n\n");
        summary.push_str(&md::table(&["Language group", "Households"], &h));
        summary.push('\n');
    }
    if !puma_languages.is_empty() {
        summary.push_str(&format!(
            "**Detailed languages (B16001) for the surrounding PUMA{} {}** — about 100,000+ residents, larger than {}:\n\n",
            if pumas.len() > 1 { "s" } else { "" },
            pumas.iter().cloned().collect::<Vec<_>>().join(", "),
            geo.name
        ));
        let d: Vec<Vec<String>> = puma_languages
            .iter()
            .map(|r| {
                vec![
                    r.language.clone(),
                    format!("{} ±{}", md::num(r.lep.value, 0), md::num(r.lep.moe.unwrap_or(0.0), 0)),
                    md::num(r.speakers.value, 0),
                ]
            })
            .collect();
        summary.push_str(&md::table(&["Language", "LEP (±MOE)", "Speakers 5+"], &d));
        summary.push('\n');
    }
    summary.push_str(&format!(
        "**Massachusetts: 15 languages most spoken by residents with LEP (B16001)**, for the \
         notice of availability in {}: {}.\n",
        rules
            .regulations
            .iter()
            .find(|r| r.citation.contains("92.11"))
            .map(|r| r.citation.as_str())
            .unwrap_or("45 CFR 92.11(b)"),
        top15
            .iter()
            .map(|d| format!("{} ({})", d.language, md::num(d.lep.value, 0)))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    let mut caveats = fig.caveats.clone();
    if let Some(c) = rules.meta.review_caveat() {
        caveats.push(c);
    }
    if sh.confidence == "low" {
        caveats.push(format!(
            "The safe harbor rule ({}) could not be checked against the published guidance while \
             this tool was built; it is marked low confidence. Confirm it before relying on it.",
            sh.citation
        ));
    }
    caveats.push(sh.limits.clone());
    caveats.extend(rules.data_limits.iter().map(|d| d.text.clone()));
    caveats.push(
        "The 15-language list is an estimate from ACS B16001 (residual \"Other ...\" groups \
         removed); HHS may publish its own state list. It is not a legal determination."
            .into(),
    );
    let mut sources = fig.sources.clone();
    sources.push(Source {
        title: rules.meta.source.title.clone(),
        publisher: rules.meta.source.publisher.clone(),
        url: rules.meta.source.url.clone(),
        effective_date: Some("2003-08-08".into()),
        retrieved: None,
        mode: SourceMode::Snapshot,
    });
    for r in &rules.regulations {
        if r.citation.contains("92.201(a)") || r.citation.contains("92.11(b)") {
            sources.push(Source {
                title: format!("{}: {}", r.citation, r.text.chars().take(90).collect::<String>() + "…"),
                publisher: "U.S. Department of Health and Human Services (eCFR)".into(),
                url: rules
                    .meta
                    .related_sources
                    .iter()
                    .find(|s| s.title.contains(r.citation.split('(').next().unwrap_or("")))
                    .and_then(|s| s.url.clone()),
                effective_date: Some("2024-07-05".into()),
                retrieved: Some("2026-11-08".into()),
                mode: SourceMode::Snapshot,
            });
        }
    }
    Ok(Envelope::new(
        summary,
        LanguageAccessResult {
            area: geo.name.clone(),
            acs_period: fig.period(),
            acs_mode: fig.mode,
            population_5plus: pop5,
            lep_total,
            lep_percent: match (lep_total, pop5) {
                (Some(l), Some(p)) if p.value > 0.0 => Some(l.value / p.value * 100.0),
                _ => None,
            },
            eligible_population: eligible.round(),
            threshold_persons: threshold.round(),
            threshold_rule: sh.rule.clone(),
            groups,
            households,
            pumas: pumas.into_iter().collect(),
            puma_languages,
            massachusetts_top_15: top15,
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
    async fn lawrence_has_spanish_above_threshold_and_pelham_none() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = language_access_profile(
            &ctx,
            LanguageAccessArgs {
                area: Some("Lawrence".into()),
                ..LanguageAccessArgs::default()
            },
        )
        .await
        .unwrap();
        let d = &env.data;
        assert_eq!(d.threshold_persons, 1000.0);
        let spanish = d.groups.iter().find(|g| g.group_id == "spanish").unwrap();
        assert_eq!(spanish.threshold_status, ThresholdStatus::ClearlyMeets);
        assert!(!d.pumas.is_empty() && !d.puma_languages.is_empty());
        assert_eq!(d.massachusetts_top_15.len(), 15);
        assert!(d.massachusetts_top_15.iter().any(|l| l.language == "Portuguese"));
        assert!(env.notice.is_some());
        assert!(env.caveats.iter().any(|c| c.contains("low confidence")));
        let env = language_access_profile(
            &ctx,
            LanguageAccessArgs {
                area: Some("Pelham".into()),
                eligible_population: Some(200.0),
                ..LanguageAccessArgs::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(env.data.threshold_persons, 10.0);
    }
}
