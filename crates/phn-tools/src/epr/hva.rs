//! EPR-06 `hazard_vulnerability_assessment`: a facilitated hazard
//! vulnerability assessment (probability × severity, where severity combines
//! impacts and preparedness gaps), seeded with the ResilientMass Plan's
//! statewide ratings, FEMA disaster declarations for the county, and — when
//! the nurse provides a FEMA National Risk Index table — NRI risk scores.

use std::collections::BTreeMap;

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::openfema;
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::alerts::declarations_for;
use super::area::{self, Area, AreaLevel};
use super::content::{self, Hva, HvaHazard, ResilientMassRatings, ScalePoint};
use crate::context::Context;
use crate::md;

/// Scores for one hazard (the planning group's judgment).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct HazardScoreInput {
    /// Hazard id or name (e.g. "winter_storms", "inland flooding", "pandemic").
    pub hazard: String,
    /// 0 = not applicable, 1 = low, 2 = moderate, 3 = high.
    pub probability: Option<u8>,
    /// Impact on people (0-3).
    pub human: Option<u8>,
    /// Impact on property and infrastructure (0-3).
    pub property: Option<u8>,
    /// Impact on health department operations and services (0-3).
    pub operations: Option<u8>,
    /// Preparedness gap: plans, training, exercises (1 = strong, 2 = moderate, 3 = weak or none).
    pub preparedness: Option<u8>,
    /// Internal response capacity gap (1-3).
    pub internal_response: Option<u8>,
    /// External partners and resources gap (1-3).
    pub external_response: Option<u8>,
}

/// Arguments for `hazard_vulnerability_assessment`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct HvaArgs {
    /// A Massachusetts city or town, or a county. Default: your saved town.
    pub area: Option<String>,
    /// The group's scores. Hazards without scores are listed with seeds only.
    pub scores: Vec<HazardScoreInput>,
    /// Use seed values (ResilientMass likelihood and human consequence) where you gave no
    /// probability or human impact (default true). Seeds are statewide starting points.
    pub use_seeds: Option<bool>,
    /// A FEMA National Risk Index table (CSV or XLSX) you downloaded into /data, county or tract level.
    pub nri_path: Option<String>,
    /// Write the facilitation workbook (XLSX) to /data/output (default true).
    pub workbook: Option<bool>,
}

/// Declaration history for a hazard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeclarationHistory {
    pub since_1953: usize,
    pub last_10_years: usize,
    pub most_recent: Option<String>,
}

/// An NRI value from the nurse's file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NriValue {
    pub prefix: String,
    pub risk_score: Option<f64>,
    pub risk_rating: Option<String>,
    pub annualized_frequency: Option<f64>,
}

/// One hazard's row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HvaRow {
    pub id: String,
    pub hazard: String,
    pub category: String,
    pub resilientmass: Option<ResilientMassRatings>,
    pub declarations: Option<DeclarationHistory>,
    pub nri: Vec<NriValue>,
    pub probability: Option<u8>,
    pub human: Option<u8>,
    pub property: Option<u8>,
    pub operations: Option<u8>,
    pub preparedness: Option<u8>,
    pub internal_response: Option<u8>,
    pub external_response: Option<u8>,
    /// Which scores came from seeds rather than the group.
    pub seeded: Vec<String>,
    /// Severity (0-1), when every item is scored.
    pub severity: Option<f64>,
    /// Relative risk (0-100%), when scored.
    pub relative_risk_percent: Option<f64>,
    pub rank: Option<usize>,
}

/// Result of `hazard_vulnerability_assessment`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HvaResult {
    pub area: Area,
    pub method: String,
    pub formula: String,
    pub hazards: Vec<HvaRow>,
    pub scored: usize,
    pub probability_scale: Vec<ScalePoint>,
    pub impact_scale: Vec<ScalePoint>,
    pub preparedness_scale: Vec<ScalePoint>,
    pub nri_file: Option<String>,
    pub nri_columns_found: Vec<String>,
    pub workbook_file: Option<String>,
}

fn check(name: &str, v: Option<u8>, lo: u8, hi: u8) -> Result<Option<u8>> {
    match v {
        Some(x) if !(lo..=hi).contains(&x) => Err(PhnError::InvalidInput(format!(
            "{name} must be {lo}-{hi} (got {x})"
        ))),
        other => Ok(other),
    }
}

fn find_hazard<'a>(c: &'a Hva, q: &str) -> Option<&'a HvaHazard> {
    let n = phn_core::text::normalize(q);
    let words: Vec<&str> = n.split_whitespace().collect();
    c.hazards
        .iter()
        .find(|h| {
            phn_core::text::normalize(&h.id) == n
                || phn_core::text::normalize(&h.id.replace('_', " ")) == n
        })
        .or_else(|| {
            c.hazards
                .iter()
                .find(|h| phn_core::text::normalize(&h.label).contains(&n))
        })
        .or_else(|| {
            let alias = match n.as_str() {
                "pandemic" | "outbreak" | "infectious disease" | "epidemic" => "infectious_disease",
                "power outage" | "power failure" | "blackout" => "extended_power_outage",
                "heat" | "extreme heat" | "heat wave" | "cold" | "extreme cold" => {
                    "extreme_temperatures"
                }
                "flood" | "flooding" | "inland flooding" | "dam failure" => "inland_flooding",
                "hurricane" | "tropical storm" => "hurricanes",
                "nor easter" | "noreaster" | "blizzard" | "snowstorm" | "winter storm"
                | "ice storm" => "winter_storms",
                "tornado" => "tornadoes",
                "wildfire" | "brush fire" => "wildfires",
                "earthquake" => "earthquakes",
                "water" | "water contamination" | "boil water" => "water_contamination",
                "cyber" | "cyberattack" | "ransomware" => "cyber",
                "chemical spill" | "hazardous materials" => "hazmat",
                "active shooter" | "mass casualty" | "terrorism" => "mass_casualty",
                _ => "",
            };
            c.hazards.iter().find(|h| h.id == alias)
        })
        .or_else(|| {
            c.hazards.iter().find(|h| {
                let l = phn_core::text::normalize(&h.label);
                !words.is_empty() && words.iter().all(|w| l.contains(w))
            })
        })
}

/// NRI values by prefix, columns found, the file's citation, and caveats.
type NriRead = (BTreeMap<String, NriValue>, Vec<String>, Source, Vec<String>);

/// Read the county (or the area's tracts, averaged) from an NRI table.
fn read_nri(ctx: &Context, c: &Hva, path: &str, area: &Area) -> Result<NriRead> {
    let lt = crate::localdata::load(ctx, path, None)?;
    let t = &lt.table;
    let up: Vec<String> = t
        .headers
        .iter()
        .map(|h| h.trim().to_ascii_uppercase())
        .collect();
    let idx = |name: &str| up.iter().position(|h| h == name);
    let county = area
        .county_fips
        .clone()
        .ok_or_else(|| PhnError::InvalidInput("NRI values need a town or county area".into()))?;
    let county_col = c.nri.county_id_columns.iter().find_map(|n| idx(n));
    let tract_col = c.nri.tract_id_columns.iter().find_map(|n| idx(n));
    let digits = |s: &str| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
    let rows: Vec<&Vec<String>> = t
        .rows
        .iter()
        .filter(|r| {
            let id = |i: Option<usize>| {
                i.and_then(|i| r.get(i))
                    .map(|v| digits(v))
                    .unwrap_or_default()
            };
            match (tract_col, county_col) {
                (Some(tc), _) if area.level == AreaLevel::Town => {
                    let tid = id(Some(tc));
                    phn_data::geography::MaGeographies::shared(&ctx.snapshots)
                        .ok()
                        .and_then(|g| g.get(&format!("{:0>11}", tid)))
                        .is_some_and(|g| g.town.as_deref() == area.town.as_deref())
                }
                (Some(tc), None) => id(Some(tc)).starts_with(&county),
                (_, Some(cc)) => format!("{:0>5}", id(Some(cc))) == county,
                (None, None) => false,
            }
        })
        .collect();
    if rows.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "no rows for {} were found in '{}'. The tool looks for a county column ({}) or a tract \
             column ({}) as in FEMA's National Risk Index tables.",
            area.name,
            lt.label,
            c.nri.county_id_columns.join(", "),
            c.nri.tract_id_columns.join(", ")
        )));
    }
    let mut prefixes: Vec<String> = c
        .hazards
        .iter()
        .flat_map(|h| h.nri_prefixes.iter().cloned())
        .collect();
    prefixes.sort();
    prefixes.dedup();
    let mut out = BTreeMap::new();
    let mut found = Vec::new();
    let num = |r: &Vec<String>, i: usize| r.get(i).and_then(|v| v.trim().parse::<f64>().ok());
    for p in &prefixes {
        let s = idx(&format!("{p}{}", c.nri.hazard_score_suffix));
        let r = idx(&format!("{p}{}", c.nri.hazard_rating_suffix));
        let f = idx(&format!("{p}{}", c.nri.hazard_frequency_suffix));
        if s.is_none() && r.is_none() && f.is_none() {
            continue;
        }
        for (col, i) in [("_RISKS", s), ("_RISKR", r), ("_AFREQ", f)] {
            if i.is_some() {
                found.push(format!("{p}{col}"));
            }
        }
        let mean = |i: Option<usize>| {
            let v: Vec<f64> = i
                .map(|i| rows.iter().filter_map(|r| num(r, i)).collect())
                .unwrap_or_default();
            (!v.is_empty())
                .then(|| (v.iter().sum::<f64>() / v.len() as f64 * 100.0).round() / 100.0)
        };
        out.insert(
            p.clone(),
            NriValue {
                prefix: p.clone(),
                risk_score: mean(s),
                risk_rating: if rows.len() == 1 {
                    r.and_then(|i| rows[0].get(i))
                        .map(|v| v.trim().to_string())
                        .filter(|v| !v.is_empty())
                } else {
                    None
                },
                annualized_frequency: mean(f),
            },
        );
    }
    let mut cav = Vec::new();
    if rows.len() > 1 {
        cav.push(format!(
            "NRI values are unweighted means over {} census tracts in {}; ratings are shown only \
             for a single county row.",
            rows.len(),
            area.name
        ));
    }
    if out.is_empty() {
        cav.push(format!(
            "No National Risk Index hazard columns (for example CFLD{}) were found in '{}'.",
            c.nri.hazard_score_suffix, lt.label
        ));
    }
    Ok((out, found, lt.source(), cav))
}

/// Run a hazard vulnerability assessment.
pub async fn hazard_vulnerability_assessment(
    ctx: &Context,
    args: HvaArgs,
) -> Result<Envelope<HvaResult>> {
    let c: Hva = content::load(content::HVA)?;
    let area = area::resolve(ctx, args.area.as_deref(), true)?;
    let use_seeds = args.use_seeds.unwrap_or(true);
    let mut caveats = Vec::new();
    let mut sources: Vec<Source> = Vec::new();

    // Inputs.
    let mut inputs: BTreeMap<String, HazardScoreInput> = BTreeMap::new();
    for s in &args.scores {
        let h = find_hazard(&c, &s.hazard).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "no hazard matches '{}'. Hazards: {}",
                s.hazard,
                c.hazards
                    .iter()
                    .map(|h| h.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
        check("probability", s.probability, 0, 3)?;
        for (n, v) in [
            ("human", s.human),
            ("property", s.property),
            ("operations", s.operations),
        ] {
            check(n, v, 0, 3)?;
        }
        for (n, v) in [
            ("preparedness", s.preparedness),
            ("internal_response", s.internal_response),
            ("external_response", s.external_response),
        ] {
            check(n, v, 1, 3)?;
        }
        inputs.insert(h.id.clone(), s.clone());
    }

    // Declarations.
    let mut decl: BTreeMap<String, DeclarationHistory> = BTreeMap::new();
    match openfema::fetch_massachusetts(&ctx.http, ctx.cache.as_ref(), &ctx.snapshots).await {
        Ok(f) => {
            let all = declarations_for(&f.declarations, &area);
            let today = chrono::Local::now().date_naive();
            let since = format!(
                "{}{}",
                chrono::Datelike::year(&today) - 10,
                today.format("-%m-%d")
            );
            for h in &c.hazards {
                if h.fema_incident_types.is_empty() {
                    continue;
                }
                let m: Vec<_> = all
                    .iter()
                    .filter(|d| h.fema_incident_types.contains(&d.incident_type))
                    .collect();
                decl.insert(
                    h.id.clone(),
                    DeclarationHistory {
                        since_1953: m.len(),
                        last_10_years: m.iter().filter(|d| d.declaration_date >= since).count(),
                        most_recent: m.iter().map(|d| d.declaration_date.clone()).max(),
                    },
                );
            }
            sources.push(match f.mode {
                SourceMode::Snapshot => ctx
                    .snapshots
                    .entry(openfema::SNAPSHOT_ID)
                    .map(|(e, _)| e.to_source())
                    .unwrap_or_else(|| Source::computed("FEMA declarations", "FEMA")),
                m => Source {
                    title: "Disaster Declarations Summaries v2 (OpenFEMA), Massachusetts".into(),
                    publisher: "Federal Emergency Management Agency".into(),
                    url: Some(openfema::DATASET_PAGE.into()),
                    effective_date: f.refreshed.clone(),
                    retrieved: Some(f.as_of.chars().take(10).collect()),
                    mode: m,
                },
            });
        }
        Err(e) => caveats.push(format!(
            "FEMA declarations could not be read ({}).",
            phn_data::apifetch::reason(&e)
        )),
    }

    // NRI.
    let (nri, nri_found, nri_file) = match &args.nri_path {
        Some(p) => {
            let (vals, found, src, cav) = read_nri(ctx, &c, p, &area)?;
            caveats.extend(cav);
            let file = src.title.clone();
            sources.push(src);
            caveats.push(c.nri.note.clone());
            (vals, found, Some(file))
        }
        None => {
            caveats.push(
                "FEMA National Risk Index data are not included: the NRI download site \
                 (hazards.fema.gov/nri) now redirects to FEMA's Resilience Analysis and Planning \
                 Tool, which could not be reached when this tool was built. If you download your \
                 county's NRI table, put it in your data folder and give nri_path."
                    .into(),
            );
            (BTreeMap::new(), Vec::new(), None)
        }
    };

    // Rows.
    let mut rows: Vec<HvaRow> = Vec::new();
    for h in &c.hazards {
        let i = inputs.get(&h.id);
        let mut seeded = Vec::new();
        let mut prob = i.and_then(|x| x.probability);
        let mut human = i.and_then(|x| x.human);
        if use_seeds && let Some(rm) = &h.resilientmass {
            if prob.is_none() {
                prob = c
                    .scales
                    .seed_mapping
                    .likelihood
                    .get(&rm.likelihood)
                    .copied();
                if prob.is_some() {
                    seeded.push("probability".to_string());
                }
            }
            if human.is_none() {
                human = c
                    .scales
                    .seed_mapping
                    .human_consequence
                    .get(&rm.human)
                    .copied();
                if human.is_some() {
                    seeded.push("human".to_string());
                }
            }
        }
        let property = i.and_then(|x| x.property);
        let operations = i.and_then(|x| x.operations);
        let prep = i.and_then(|x| x.preparedness);
        let int = i.and_then(|x| x.internal_response);
        let ext = i.and_then(|x| x.external_response);
        let items = [human, property, operations, prep, int, ext];
        let severity = items
            .iter()
            .all(Option::is_some)
            .then(|| items.iter().map(|v| v.unwrap_or(0) as f64).sum::<f64>() / 18.0);
        let risk = match (prob, severity) {
            (Some(p), Some(s)) => Some(((p as f64 / 3.0) * s * 1000.0).round() / 10.0),
            _ => None,
        };
        rows.push(HvaRow {
            id: h.id.clone(),
            hazard: h.label.clone(),
            category: h.category.clone(),
            resilientmass: h.resilientmass.clone(),
            declarations: decl.get(&h.id).cloned(),
            nri: h
                .nri_prefixes
                .iter()
                .filter_map(|p| nri.get(p).cloned())
                .collect(),
            probability: prob,
            human,
            property,
            operations,
            preparedness: prep,
            internal_response: int,
            external_response: ext,
            seeded,
            severity: severity.map(|s| (s * 1000.0).round() / 1000.0),
            relative_risk_percent: risk,
            rank: None,
        });
    }
    // Rank scored hazards.
    let mut order: Vec<usize> = (0..rows.len())
        .filter(|&i| rows[i].relative_risk_percent.is_some())
        .collect();
    order.sort_by(|&a, &b| {
        rows[b]
            .relative_risk_percent
            .partial_cmp(&rows[a].relative_risk_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(rows[b].probability.cmp(&rows[a].probability))
    });
    for (r, &i) in order.iter().enumerate() {
        rows[i].rank = Some(r + 1);
    }
    let scored = order.len();
    // Present ranked first, then the rest in list order.
    let mut presented: Vec<HvaRow> = order.iter().map(|&i| rows[i].clone()).collect();
    presented.extend(rows.iter().filter(|r| r.rank.is_none()).cloned());

    // Workbook.
    let mut files_written = Vec::new();
    let mut workbook_file = None;
    if args.workbook.unwrap_or(true) {
        let fmt = |v: Option<u8>| v.map(|x| x.to_string()).unwrap_or_default();
        let table_rows: Vec<Vec<String>> = presented
            .iter()
            .map(|r| {
                vec![
                    r.hazard.clone(),
                    r.resilientmass
                        .as_ref()
                        .map(|m| m.likelihood.clone())
                        .unwrap_or_default(),
                    r.resilientmass
                        .as_ref()
                        .map(|m| m.human.clone())
                        .unwrap_or_default(),
                    r.resilientmass
                        .as_ref()
                        .map(|m| m.warning.clone())
                        .unwrap_or_default(),
                    r.declarations
                        .as_ref()
                        .map(|d| d.since_1953.to_string())
                        .unwrap_or_default(),
                    r.nri
                        .iter()
                        .filter_map(|n| n.risk_score.map(|s| format!("{} {s}", n.prefix)))
                        .collect::<Vec<_>>()
                        .join("; "),
                    fmt(r.probability),
                    fmt(r.human),
                    fmt(r.property),
                    fmt(r.operations),
                    fmt(r.preparedness),
                    fmt(r.internal_response),
                    fmt(r.external_response),
                    r.relative_risk_percent
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                    r.rank.map(|v| v.to_string()).unwrap_or_default(),
                    String::new(),
                ]
            })
            .collect();
        let scale_rows = |s: &[ScalePoint]| -> Vec<Vec<String>> {
            s.iter()
                .map(|p| vec![p.score.to_string(), p.label.clone(), p.description.clone()])
                .collect()
        };
        let doc = Document {
            title: format!("Hazard vulnerability assessment — {}", area.name),
            subtitle: Some("Facilitation workbook".into()),
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks: vec![
                Block::Paragraph(vec![Inline::plain(format!(
                    "{} Formula: {}",
                    c.method.summary, c.method.formula
                ))]),
                Block::Table(TableBlock {
                    caption: Some("Hazards".into()),
                    headers: [
                        "Hazard",
                        "ResilientMass likelihood (statewide)",
                        "ResilientMass human consequence",
                        "Warning time",
                        "FEMA declarations since 1953",
                        "NRI risk score",
                        "Probability (0-3)",
                        "Human (0-3)",
                        "Property (0-3)",
                        "Operations (0-3)",
                        "Preparedness gap (1-3)",
                        "Internal response gap (1-3)",
                        "External response gap (1-3)",
                        "Relative risk (%)",
                        "Rank",
                        "Group notes",
                    ]
                    .map(String::from)
                    .to_vec(),
                    rows: table_rows,
                }),
                Block::Table(TableBlock {
                    caption: Some("Probability scale".into()),
                    headers: vec!["Score".into(), "Label".into(), "Meaning".into()],
                    rows: scale_rows(&c.scales.probability),
                }),
                Block::Table(TableBlock {
                    caption: Some("Impact scale".into()),
                    headers: vec!["Score".into(), "Label".into(), "Meaning".into()],
                    rows: scale_rows(&c.scales.impact),
                }),
                Block::Table(TableBlock {
                    caption: Some("Preparedness gap scale".into()),
                    headers: vec!["Score".into(), "Label".into(), "Meaning".into()],
                    rows: scale_rows(&c.scales.preparedness_gap),
                }),
            ],
        };
        let bytes = phn_docs::xlsx::to_xlsx(&doc)?;
        let sandbox = ctx.sandbox()?;
        let path = sandbox.new_output_file("hazard_vulnerability_assessment.xlsx")?;
        std::fs::write(&path, bytes)?;
        let disp = sandbox.display_path(&path);
        files_written.push(disp.clone());
        workbook_file = Some(disp);
    }

    // Summary.
    let mut s = format!("**Hazard vulnerability assessment — {}**\n\n", area.name);
    if scored > 0 {
        s.push_str(&format!(
            "{scored} hazard(s) scored. Ranked by relative risk:\n\n"
        ));
        let tr: Vec<Vec<String>> = presented
            .iter()
            .filter(|r| r.rank.is_some())
            .map(|r| {
                vec![
                    r.rank.unwrap_or_default().to_string(),
                    r.hazard.clone(),
                    r.probability.map(|v| v.to_string()).unwrap_or_default(),
                    r.severity
                        .map(|v| format!("{:.0}%", v * 100.0))
                        .unwrap_or_default(),
                    format!("{}%", r.relative_risk_percent.unwrap_or_default()),
                    if r.seeded.is_empty() {
                        String::new()
                    } else {
                        format!("seeded: {}", r.seeded.join(", "))
                    },
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "Rank",
                "Hazard",
                "Probability",
                "Severity",
                "Relative risk",
                "Note",
            ],
            &tr,
        ));
    } else {
        s.push_str(
            "No hazard has every score yet. Score probability (0-3), impacts on people, property, \
             and operations (0-3), and preparedness, internal response, and external response \
             gaps (1-3) for each hazard; the workbook lists the scales.\n",
        );
    }
    s.push_str("\n**Seeds for discussion** (statewide ResilientMass ratings and FEMA declarations for the area)\n\n");
    let seed_rows: Vec<Vec<String>> = presented
        .iter()
        .map(|r| {
            vec![
                r.hazard.clone(),
                r.resilientmass
                    .as_ref()
                    .map(|m| format!("{} / {}", m.likelihood, m.human))
                    .unwrap_or_else(|| "—".into()),
                r.resilientmass
                    .as_ref()
                    .map(|m| m.warning.clone())
                    .unwrap_or_else(|| "—".into()),
                r.declarations
                    .as_ref()
                    .map(|d| {
                        format!(
                            "{} ({} in 10 years){}",
                            d.since_1953,
                            d.last_10_years,
                            d.most_recent
                                .as_deref()
                                .map(|x| format!(", last {x}"))
                                .unwrap_or_default()
                        )
                    })
                    .unwrap_or_else(|| "—".into()),
                if r.nri.is_empty() {
                    "—".into()
                } else {
                    r.nri
                        .iter()
                        .map(|n| {
                            format!(
                                "{} {}{}",
                                n.prefix,
                                n.risk_score.map(|v| v.to_string()).unwrap_or_default(),
                                n.risk_rating
                                    .as_deref()
                                    .map(|x| format!(" ({x})"))
                                    .unwrap_or_default()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("; ")
                },
            ]
        })
        .collect();
    s.push_str(&md::table(
        &[
            "Hazard",
            "ResilientMass likelihood / human consequence",
            "Warning time",
            "FEMA declarations since 1953",
            "NRI",
        ],
        &seed_rows,
    ));
    s.push_str(&format!("\n_Method:_ {}\n", c.method.summary));
    if let Some(f) = &workbook_file {
        s.push_str(&format!("\nFacilitation workbook written to `{f}`.\n"));
    }

    caveats.push(c.method.note.clone());
    caveats.push(
        "ResilientMass ratings are statewide; local probability and impact can differ a great \
         deal (for example coastal flooding inland). Treat seeds as starting points for the \
         group's discussion."
            .into(),
    );
    caveats.push(
        "FEMA declarations count federally declared events only; many local floods, storms, and \
         heat waves were never declared."
            .into(),
    );
    caveats.extend(content::review_caveats(&[content::HVA]));
    sources.extend(content::sources_for(
        c.sources.keys().map(String::as_str),
        &c.sources,
    ));

    Ok(Envelope::new(
        s,
        HvaResult {
            area,
            method: c.method.summary.clone(),
            formula: c.method.formula.clone(),
            hazards: presented,
            scored,
            probability_scale: c.scales.probability.clone(),
            impact_scale: c.scales.impact.clone(),
            preparedness_scale: c.scales.preparedness_gap.clone(),
            nri_file,
            nri_columns_found: nri_found,
            workbook_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        files_written,
        ..Privacy::none()
    }))
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

    #[tokio::test]
    async fn scores_rank_and_seed() {
        let (dir, c) = ctx();
        std::fs::write(
            dir.path().join("data/nri.csv"),
            "STCOFIPS,COUNTY,RISK_SCORE,CFLD_RISKS,CFLD_RISKR,RFLD_RISKS,HWAV_AFREQ\n25005,Bristol,90.1,95.2,Very High,80.5,1.2\n25027,Worcester,70,10,Low,60,1.1\n",
        )
        .unwrap();
        let env = hazard_vulnerability_assessment(
            &c,
            HvaArgs {
                area: Some("Bristol County".into()),
                scores: vec![
                    HazardScoreInput {
                        hazard: "nor'easter".into(),
                        probability: Some(3),
                        human: Some(2),
                        property: Some(2),
                        operations: Some(2),
                        preparedness: Some(1),
                        internal_response: Some(2),
                        external_response: Some(2),
                    },
                    HazardScoreInput {
                        hazard: "pandemic".into(),
                        probability: Some(2),
                        human: Some(3),
                        property: Some(1),
                        operations: Some(3),
                        preparedness: Some(2),
                        internal_response: Some(2),
                        external_response: Some(2),
                    },
                    HazardScoreInput {
                        hazard: "coastal flooding".into(),
                        property: Some(3),
                        operations: Some(1),
                        preparedness: Some(2),
                        internal_response: Some(1),
                        external_response: Some(1),
                        ..Default::default()
                    },
                ],
                nri_path: Some("nri.csv".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let d = &env.data;
        assert_eq!(d.scored, 3);
        let row = |id: &str| d.hazards.iter().find(|r| r.id == id).unwrap();
        // Winter storms: (3/3) × (2+2+2+1+2+2)/18 = 61.1%.
        assert_eq!(row("winter_storms").relative_risk_percent, Some(61.1));
        // Pandemic: (2/3) × 13/18 = 48.1%.
        assert_eq!(row("infectious_disease").relative_risk_percent, Some(48.1));
        // Coastal flooding seeds probability 3 (Very high) and human 2 (High): (3/3) × 10/18 = 55.6%.
        let cf = row("coastal_flooding");
        assert_eq!(cf.seeded, vec!["probability", "human"]);
        assert_eq!(cf.relative_risk_percent, Some(55.6));
        assert_eq!(d.hazards[0].id, "winter_storms");
        assert_eq!(d.hazards[1].id, "coastal_flooding");
        assert_eq!(cf.nri[0].risk_score, Some(95.2));
        assert_eq!(cf.nri[0].risk_rating.as_deref(), Some("Very High"));
        assert!(row("hurricanes").declarations.as_ref().unwrap().since_1953 > 5);
        assert!(d.workbook_file.as_ref().unwrap().ends_with(".xlsx"));
        assert!(d.nri_columns_found.contains(&"CFLD_RISKS".to_string()));
        assert!(
            hazard_vulnerability_assessment(
                &c,
                HvaArgs {
                    area: Some("Bristol County".into()),
                    scores: vec![HazardScoreInput {
                        hazard: "volcano".into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }
            )
            .await
            .is_err()
        );
    }
}
