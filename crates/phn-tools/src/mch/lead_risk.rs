//! MCH-05 `map_lead_risk`.
//!
//! Ranks Massachusetts cities and towns (or census tracts) for childhood lead outreach from
//! old housing (ACS B25034: built before 1950 and before 1980), low income (ACS: share below
//! 200% of poverty), young children (ACS), DPH's published screening rates and blood lead
//! prevalence (2024 surveillance report), and DPH's high-risk community designation, and draws
//! a choropleth map with the existing renderer.
//!
//! The ranking is this server's transparent outreach index (the mean of percentile ranks of
//! the chosen indicators among the areas in scope), not a DPH method; DPH's own high-risk
//! score is shown for designated communities.

use std::collections::BTreeMap;

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::census::GeoLevel;
use phn_data::geography::MaGeographies;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::lead_data::{self, LeadTownRow};
use crate::context::Context;
use crate::epi::choropleth::{
    AreaValue, CreateChoroplethArgs, Extent, MapGeography, Method, create_choropleth_map,
};
use crate::md;

/// Geography to rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LeadGeography {
    /// Cities and towns (default): all indicators, including DPH screening data.
    #[default]
    Town,
    /// 2020 census tracts: ACS housing, income, and young children; the containing town's DPH
    /// designation is shown.
    Tract,
}

/// Indicators for the outreach ranking.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LeadIndicator {
    /// Percent of housing units built before 1950 (ACS B25034).
    Pre1950Housing,
    /// Percent of housing units built before 1980 (ACS B25034).
    Pre1980Housing,
    /// Percent of people below 200% of the poverty level (ACS).
    LowIncome,
    /// Percent of the population under 5 (ACS).
    ChildrenUnder5,
    /// 100 minus the percent of children 9-47 months screened (DPH, towns only).
    ScreeningGap,
    /// Percent of screened children with an estimated confirmed level of 5 ug/dL or more (DPH,
    /// towns only).
    ElevatedBll,
}

impl LeadIndicator {
    fn label(self) -> &'static str {
        match self {
            Self::Pre1950Housing => "% housing built before 1950",
            Self::Pre1980Housing => "% housing built before 1980",
            Self::LowIncome => "% below 200% of poverty",
            Self::ChildrenUnder5 => "% population under 5",
            Self::ScreeningGap => "% of children 9-47 months not screened",
            Self::ElevatedBll => "% screened with BLL 5+ (est. confirmed)",
        }
    }

    fn town_only(self) -> bool {
        matches!(self, Self::ScreeningGap | Self::ElevatedBll)
    }
}

/// Arguments for `map_lead_risk`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct LeadRiskArgs {
    /// town (default) or tract.
    #[serde(default)]
    pub geography: LeadGeography,
    /// Limit to one county ("Bristol" or "Bristol County").
    #[serde(default)]
    pub county: Option<String>,
    /// Limit to these towns (for tracts: tracts whose main town is listed).
    #[serde(default)]
    pub towns: Vec<String>,
    /// Indicators in the ranking (default: pre1950_housing, low_income, and for towns
    /// screening_gap and elevated_bll).
    #[serde(default)]
    pub indicators: Vec<LeadIndicator>,
    /// Number of areas to list (default 20, at most 100).
    #[serde(default)]
    pub top: Option<usize>,
    /// Draw a choropleth map of the outreach index (default true). Needs /data mounted.
    #[serde(default)]
    pub map: Option<bool>,
}

/// One ranked area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RankedArea {
    pub rank: usize,
    pub geoid: String,
    pub name: String,
    /// Main town (tracts).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub town: Option<String>,
    /// Outreach index: mean percentile rank (0-100) of the indicators among areas in scope.
    pub outreach_index: f64,
    /// Indicator values (label -> value).
    pub values: BTreeMap<String, Option<f64>>,
    /// On DPH's current high-risk community list (the town, for tracts).
    pub dph_high_risk: bool,
    /// DPH's high-risk score (designated communities only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dph_high_risk_score: Option<f64>,
    /// Year of the DPH screening figures (towns).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screening_data_year: Option<String>,
}

/// Result of `map_lead_risk`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LeadRiskResult {
    pub geography: LeadGeography,
    pub areas_ranked: usize,
    pub indicators: Vec<String>,
    pub method: String,
    pub ranked: Vec<RankedArea>,
    /// DPH high-risk communities in scope that are not in the top list.
    pub dph_high_risk_not_in_top: Vec<String>,
    /// Statewide values for comparison.
    pub state_values: BTreeMap<String, Option<f64>>,
    /// Map files (SVG/PNG) in the output folder, when drawn.
    pub map_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_alt_text: Option<String>,
}

fn parse(v: &str) -> Option<f64> {
    v.trim().parse::<f64>().ok()
}

/// ACS rows by GEOID: (poverty200, under5) from the indicator snapshot, and housing age.
fn acs(ctx: &Context) -> Result<(BTreeMap<String, [Option<f64>; 4]>, Vec<Source>)> {
    let (ind, ind_entry, _) = ctx
        .snapshots
        .file_text("census-acs5-ma", "ma_acs5_indicators.csv")?;
    let (hous, hous_entry, _) = ctx
        .snapshots
        .file_text("ma-acs5-housing-age", "ma_acs5_housing_age.csv")?;
    let mut out: BTreeMap<String, [Option<f64>; 4]> = BTreeMap::new();
    let mut rdr = csv::Reader::from_reader(ind.as_bytes());
    let h = rdr
        .headers()
        .map_err(|e| PhnError::Internal(format!("ACS snapshot: {e}")))?
        .clone();
    let col = |n: &str| h.iter().position(|x| x == n);
    let (cg, cp, cu) = (col("geoid"), col("poverty200_pct"), col("age_under5_pct"));
    let (Some(cg), Some(cp), Some(cu)) = (cg, cp, cu) else {
        return Err(PhnError::Internal(
            "ACS snapshot lacks expected columns".into(),
        ));
    };
    for r in rdr.records() {
        let r = r.map_err(|e| PhnError::Internal(format!("ACS snapshot: {e}")))?;
        let e = out.entry(r[cg].to_string()).or_insert([None; 4]);
        e[2] = parse(&r[cp]);
        e[3] = parse(&r[cu]);
    }
    let mut rdr = csv::Reader::from_reader(hous.as_bytes());
    for r in rdr.records() {
        let r = r.map_err(|e| PhnError::Internal(format!("housing snapshot: {e}")))?;
        let e = out.entry(r[0].to_string()).or_insert([None; 4]);
        e[0] = parse(&r[7]);
        e[1] = parse(&r[11]);
    }
    Ok((out, vec![hous_entry.to_source(), ind_entry.to_source()]))
}

/// Percentile ranks (0-100, ties averaged) of present values.
fn percentile_ranks(values: &[Option<f64>]) -> Vec<Option<f64>> {
    let mut present: Vec<(usize, f64)> = values
        .iter()
        .enumerate()
        .filter_map(|(i, v)| v.map(|x| (i, x)))
        .collect();
    present.sort_by(|a, b| a.1.total_cmp(&b.1));
    let n = present.len();
    let mut out = vec![None; values.len()];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && present[j + 1].1 == present[i].1 {
            j += 1;
        }
        let avg = (i + j) as f64 / 2.0;
        let pr = if n > 1 {
            avg / (n - 1) as f64 * 100.0
        } else {
            50.0
        };
        for k in i..=j {
            out[present[k].0] = Some(pr);
        }
        i = j + 1;
    }
    out
}

/// Rank areas for lead outreach and map them.
pub fn map_lead_risk(ctx: &Context, args: LeadRiskArgs) -> Result<Envelope<LeadRiskResult>> {
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let (lead_rows, lead_source) = lead_data::load(ctx)?;
    let (acs, mut sources) = acs(ctx)?;
    let (hr_towns, hr_year, hr_method) = super::lead::high_risk_towns();
    let lead_by_town: BTreeMap<&str, &LeadTownRow> =
        lead_rows.iter().map(|r| (r.town.as_str(), r)).collect();
    let top = args.top.unwrap_or(20);
    if top == 0 || top > 100 {
        return Err(PhnError::InvalidInput(
            "top should be between 1 and 100".into(),
        ));
    }
    let mut indicators = if args.indicators.is_empty() {
        let mut v = vec![LeadIndicator::Pre1950Housing, LeadIndicator::LowIncome];
        if args.geography == LeadGeography::Town {
            v.extend([LeadIndicator::ScreeningGap, LeadIndicator::ElevatedBll]);
        }
        v
    } else {
        args.indicators.clone()
    };
    indicators.sort();
    indicators.dedup();
    if args.geography == LeadGeography::Tract
        && let Some(bad) = indicators.iter().find(|i| i.town_only())
    {
        return Err(PhnError::InvalidInput(format!(
            "'{}' is published by DPH for cities and towns only; use geography \"town\" or \
             choose pre1950_housing, pre1980_housing, low_income, children_under5",
            bad.label()
        )));
    }
    let county =
        match &args.county {
            Some(c) => Some(geos.county(c).ok_or_else(|| {
                PhnError::NotFound(format!("'{c}' is not a Massachusetts county"))
            })?),
            None => None,
        };
    let mut towns_filter = Vec::new();
    for t in &args.towns {
        let m = ctx.ma.town(t).ok_or_else(|| {
            let sugg: Vec<String> = ctx
                .ma
                .suggest_towns(t, 3)
                .iter()
                .map(|m| m.name.clone())
                .collect();
            PhnError::NotFound(format!(
                "'{t}' is not a Massachusetts city or town{}",
                if sugg.is_empty() {
                    String::new()
                } else {
                    format!(". Did you mean {}?", sugg.join(", "))
                }
            ))
        })?;
        towns_filter.push(m.name.clone());
    }
    let level = match args.geography {
        LeadGeography::Town => GeoLevel::Town,
        LeadGeography::Tract => GeoLevel::Tract,
    };
    // Areas in scope.
    let areas: Vec<_> = geos
        .at_level(level)
        .filter(|g| county.is_none_or(|c| g.county_geoid == c.geoid))
        .filter(|g| {
            towns_filter.is_empty()
                || match args.geography {
                    LeadGeography::Town => towns_filter.contains(&g.name),
                    LeadGeography::Tract => {
                        g.town.as_ref().is_some_and(|t| towns_filter.contains(t))
                    }
                }
        })
        .collect();
    if areas.len() < 2 {
        return Err(PhnError::InvalidInput(
            "fewer than 2 areas are in scope; widen the county or town filter".into(),
        ));
    }
    let acs_value = |geoid: &str, ind: LeadIndicator| -> Option<f64> {
        let a = acs.get(geoid);
        match ind {
            LeadIndicator::Pre1950Housing => a.and_then(|x| x[0]),
            LeadIndicator::Pre1980Housing => a.and_then(|x| x[1]),
            LeadIndicator::LowIncome => a.and_then(|x| x[2]),
            LeadIndicator::ChildrenUnder5 => a.and_then(|x| x[3]),
            _ => None,
        }
    };
    let value_of = |g: &phn_data::geography::Geography, ind: LeadIndicator| -> Option<f64> {
        match ind {
            LeadIndicator::Pre1950Housing
            | LeadIndicator::Pre1980Housing
            | LeadIndicator::LowIncome
            | LeadIndicator::ChildrenUnder5 => acs_value(&g.geoid, ind),
            LeadIndicator::ScreeningGap => lead_by_town
                .get(g.name.as_str())
                .and_then(|r| LeadTownRow::num(&r.pct_screened))
                .map(|p| (100.0 - p).max(0.0)),
            LeadIndicator::ElevatedBll => lead_by_town
                .get(g.name.as_str())
                .and_then(|r| LeadTownRow::num(&r.est_confirmed_5plus_pct)),
        }
    };
    let matrix: Vec<Vec<Option<f64>>> = indicators
        .iter()
        .map(|ind| areas.iter().map(|g| value_of(g, *ind)).collect())
        .collect();
    let ranks: Vec<Vec<Option<f64>>> = matrix.iter().map(|col| percentile_ranks(col)).collect();
    let mut scored: Vec<(usize, f64)> = (0..areas.len())
        .filter_map(|i| {
            let present: Vec<f64> = ranks.iter().filter_map(|r| r[i]).collect();
            (!present.is_empty()).then(|| (i, present.iter().sum::<f64>() / present.len() as f64))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then(areas[a.0].name.cmp(&areas[b.0].name))
    });
    let town_of = |g: &phn_data::geography::Geography| -> String {
        match args.geography {
            LeadGeography::Town => g.name.clone(),
            LeadGeography::Tract => g.town.clone().unwrap_or_default(),
        }
    };
    let ranked: Vec<RankedArea> = scored
        .iter()
        .take(top)
        .enumerate()
        .map(|(k, (i, score))| {
            let g = areas[*i];
            let t = town_of(g);
            let lr = lead_by_town.get(t.as_str());
            RankedArea {
                rank: k + 1,
                geoid: g.geoid.clone(),
                name: g.name.clone(),
                town: (args.geography == LeadGeography::Tract).then(|| t.clone()),
                outreach_index: *score,
                values: indicators
                    .iter()
                    .enumerate()
                    .map(|(j, ind)| (ind.label().to_string(), matrix[j][*i]))
                    .collect(),
                dph_high_risk: hr_towns.iter().any(|h| *h == t),
                dph_high_risk_score: lr.and_then(|r| parse(&r.hr_score)),
                screening_data_year: (args.geography == LeadGeography::Town)
                    .then(|| lr.map(|r| r.data_year.clone()))
                    .flatten(),
            }
        })
        .collect();
    let in_scope_towns: Vec<String> = areas.iter().map(|g| town_of(g)).collect();
    let listed: Vec<&str> = ranked
        .iter()
        .map(|r| r.town.as_deref().unwrap_or(&r.name))
        .collect();
    let mut not_in_top: Vec<String> = hr_towns
        .iter()
        .filter(|h| in_scope_towns.contains(h) && !listed.contains(&h.as_str()))
        .cloned()
        .collect();
    not_in_top.dedup();
    let state_values: BTreeMap<String, Option<f64>> = indicators
        .iter()
        .map(|ind| {
            let v = match ind {
                LeadIndicator::ScreeningGap => lead_by_town
                    .get("Total for MA")
                    .and_then(|r| LeadTownRow::num(&r.pct_screened))
                    .map(|p| 100.0 - p),
                LeadIndicator::ElevatedBll => lead_by_town
                    .get("Total for MA")
                    .and_then(|r| LeadTownRow::num(&r.est_confirmed_5plus_pct)),
                _ => acs_value("25", *ind),
            };
            (ind.label().to_string(), v)
        })
        .collect();
    let method = format!(
        "Outreach index = mean of percentile ranks (0-100, ties averaged) of: {}, among the {} \
         {} in scope; higher = more outreach need. An area missing an indicator is ranked on \
         the others. This is this server's ranking, not a DPH method. DPH's high-risk list \
         ({hr_year}): {hr_method}",
        indicators
            .iter()
            .map(|i| i.label())
            .collect::<Vec<_>>()
            .join("; "),
        scored.len(),
        match args.geography {
            LeadGeography::Town => "cities and towns",
            LeadGeography::Tract => "census tracts",
        }
    );

    // Map of the outreach index.
    let mut map_files = Vec::new();
    let mut alt = None;
    let mut caveats = Vec::new();
    if args.map.unwrap_or(true) {
        let values: Vec<AreaValue> = scored
            .iter()
            .map(|(i, s)| AreaValue {
                area: match args.geography {
                    LeadGeography::Town => areas[*i].name.clone(),
                    LeadGeography::Tract => areas[*i].geoid.clone(),
                },
                value: Some((s * 10.0).round() / 10.0),
                count: None,
            })
            .collect();
        let res = create_choropleth_map(
            ctx,
            CreateChoroplethArgs {
                geography: match args.geography {
                    LeadGeography::Town => MapGeography::Town,
                    LeadGeography::Tract => MapGeography::Tract,
                },
                title: "Childhood lead outreach index".into(),
                values: Some(values),
                path: None,
                sheet: None,
                area_column: None,
                value_column: None,
                count_column: None,
                value_is_count: Some(false),
                subtitle: Some(
                    indicators
                        .iter()
                        .map(|i| i.label())
                        .collect::<Vec<_>>()
                        .join("; "),
                ),
                value_label: Some("Outreach index (0-100)".into()),
                method: Some(Method::Quantile),
                classes: Some(5),
                extent: Some(Extent::Data),
                labels: None,
                decimals: Some(0),
                source_note: Some(
                    "ACS 2020-2024; DPH 2024 childhood lead surveillance report".into(),
                ),
                file_name: Some(match args.geography {
                    LeadGeography::Town => "lead_outreach_index_towns".into(),
                    LeadGeography::Tract => "lead_outreach_index_tracts".into(),
                }),
            },
        );
        match res {
            Ok(env) => {
                map_files = env.data.files.clone();
                alt = Some(env.data.alt_text.clone());
            }
            Err(e) => caveats.push(format!("The map was not drawn: {e}")),
        }
    }

    // ---- Markdown
    let mut s = format!(
        "**Childhood lead outreach ranking — {} {}{}**\n\n",
        scored.len(),
        match args.geography {
            LeadGeography::Town => "cities and towns",
            LeadGeography::Tract => "census tracts",
        },
        county
            .map(|c| format!(" in {}", c.name))
            .unwrap_or_default()
    );
    let mut headers: Vec<String> = vec!["#".into(), "Area".into(), "Index".into()];
    headers.extend(indicators.iter().map(|i| i.label().to_string()));
    headers.push("DPH high-risk".into());
    let rows: Vec<Vec<String>> = ranked
        .iter()
        .map(|r| {
            let mut v = vec![
                r.rank.to_string(),
                match &r.town {
                    Some(t) => format!("{} ({t})", r.name),
                    None => r.name.clone(),
                },
                format!("{:.0}", r.outreach_index),
            ];
            v.extend(indicators.iter().map(|i| {
                r.values
                    .get(i.label())
                    .copied()
                    .flatten()
                    .map(|x| format!("{x:.1}"))
                    .unwrap_or_else(|| "NS".into())
            }));
            v.push(if r.dph_high_risk {
                match r.dph_high_risk_score {
                    Some(sc) => format!("yes (score {sc})"),
                    None => "yes".into(),
                }
            } else {
                String::new()
            });
            v
        })
        .collect();
    let hdr: Vec<&str> = headers.iter().map(String::as_str).collect();
    s.push_str(&md::table(&hdr, &rows));
    s.push_str(&format!(
        "\nMassachusetts: {}.\n",
        state_values
            .iter()
            .map(|(k, v)| format!(
                "{k} {}",
                v.map(|x| format!("{x:.1}")).unwrap_or_else(|| "n/a".into())
            ))
            .collect::<Vec<_>>()
            .join("; ")
    ));
    if !not_in_top.is_empty() {
        s.push_str(&format!(
            "\nDPH high-risk communities in scope that are not in this top {top}: {}.\n",
            not_in_top.join(", ")
        ));
    }
    if !map_files.is_empty() {
        s.push_str(&format!(
            "\nMap written to {}.\n",
            map_files
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(" and ")
        ));
    }
    s.push_str(&format!("\n_Method: {method}_\n"));

    caveats.push(
        "The outreach index ranks areas relative to each other for planning; it is not a \
         measure of risk for any child. Every child in Massachusetts must be screened at 9-12 \
         months, 2, and 3 years regardless of where they live."
            .into(),
    );
    caveats.push(
        "ACS figures are 2020-2024 5-year estimates with margins of error (large for tracts); \
         ACS year-built bins do not split at 1978, so pre-1980 stands in for the Lead Law's \
         pre-1978. Values come from bundled snapshots (the Census API needs a key from some \
         networks; the same vintage is bundled)."
            .into(),
    );
    if indicators.iter().any(|i| i.town_only()) {
        caveats.push(
            "DPH screening figures are for 2024 except 24 communities (Monson to Northampton, \
             including New Bedford) shown with 2023 values because DPH's 2024 report omitted \
             their page. 'NS' marks values DPH suppressed (1-5 children) and areas ranked on \
             the remaining indicators."
                .into(),
        );
    }
    sources.push(lead_source);
    let data = LeadRiskResult {
        geography: args.geography,
        areas_ranked: scored.len(),
        indicators: indicators.iter().map(|i| i.label().to_string()).collect(),
        method,
        ranked,
        dph_high_risk_not_in_top: not_in_top,
        state_values,
        map_files: map_files.clone(),
        map_alt_text: alt,
    };
    let mut privacy = Privacy::none();
    privacy.files_written = map_files;
    Ok(Envelope::new(s, data)
        .sources(sources)
        .source(Source::computed(
            "Outreach index (mean percentile rank of indicators)",
            "PHN MCP server",
        ))
        .caveats(caveats)
        .privacy(privacy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn percentile_ranks_handle_ties_and_missing() {
        let r = percentile_ranks(&[Some(1.0), None, Some(3.0), Some(3.0), Some(5.0)]);
        assert_eq!(r[0], Some(0.0));
        assert_eq!(r[1], None);
        assert_eq!(r[2], Some(50.0));
        assert_eq!(r[3], Some(50.0));
        assert_eq!(r[4], Some(100.0));
    }

    #[test]
    fn statewide_town_ranking_puts_high_risk_cities_near_the_top() {
        let (_d, c) = ctx();
        let env = map_lead_risk(
            &c,
            LeadRiskArgs {
                top: Some(40),
                map: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &env.data;
        assert_eq!(d.areas_ranked, 351);
        assert_eq!(d.ranked.len(), 40);
        let hr = d.ranked.iter().filter(|r| r.dph_high_risk).count();
        assert!(hr >= 8, "only {hr} DPH high-risk towns in the top 40");
        assert!(!d.map_files.is_empty());
        assert!(env.summary.contains("Outreach index"));
    }

    #[test]
    fn tracts_in_a_county_and_errors() {
        let (_d, c) = ctx();
        let env = map_lead_risk(
            &c,
            LeadRiskArgs {
                geography: LeadGeography::Tract,
                county: Some("Bristol".into()),
                top: Some(10),
                map: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(env.data.areas_ranked > 50);
        assert!(env.data.ranked[0].town.is_some());
        let e = map_lead_risk(
            &c,
            LeadRiskArgs {
                geography: LeadGeography::Tract,
                indicators: vec![LeadIndicator::ScreeningGap],
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("cities and towns only"));
    }
}
