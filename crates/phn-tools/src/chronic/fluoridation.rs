//! CHR-07 `community_water_fluoridation_status`: fluoridation status of a
//! Massachusetts town (DPH 2023 list; CDC My Water's Fluoride 2025 system
//! report), a statewide summary, each body's current position and the evidence
//! side by side, M.G.L. c. 111, § 8C, and board of health discussion points,
//! with an optional DOCX brief.

use std::collections::BTreeMap;

use phn_core::{Envelope, Privacy, Result, Source};
use phn_data::water::{DphTownStatus, FluoridationData, MwfSystem};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::area::{self, Area, AreaLevel};
use crate::epr::content::{self, Cited, CitedOut, Confidence, DocSource};
use crate::md;

/// Content id of `content/chronic/fluoridation.yaml`.
pub const FLUORIDATION: &str = "chronic-fluoridation";

// ------------------------------------------------------------------ content

#[derive(Debug, Clone, Deserialize)]
pub struct PositionDef {
    pub body: String,
    pub kind: String,
    pub date: String,
    pub position: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvidenceDef {
    pub review: String,
    pub finding: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LinkDef {
    pub label: String,
    pub url: String,
}

/// `content/chronic/fluoridation.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct FluoridationContent {
    pub sources: BTreeMap<String, DocSource>,
    pub standards: Vec<Cited>,
    pub status_definitions: Vec<Cited>,
    pub positions: Vec<PositionDef>,
    pub evidence: Vec<EvidenceDef>,
    pub divergences: Vec<Cited>,
    pub ma_law: Vec<Cited>,
    pub discussion_points: Vec<Cited>,
    pub not_included: Vec<String>,
    pub links: Vec<LinkDef>,
}

// --------------------------------------------------------------------- args

/// Arguments for `community_water_fluoridation_status`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct FluoridationArgs {
    /// A Massachusetts city or town, or "Massachusetts" for the statewide summary only.
    /// Default: your saved town; otherwise the statewide summary.
    pub town: Option<String>,
    /// Write a DOCX brief for a board of health discussion to /data/output (default false).
    pub brief: Option<bool>,
}

// ------------------------------------------------------------------- output

/// One row of the DPH 2023 list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DphRow {
    pub status: String,
    pub population: Option<u64>,
    pub year_began: Option<String>,
    /// Receives fluoridated water from the MWRA.
    pub mwra: bool,
    /// DPH's additional information, verbatim.
    pub information: String,
}

/// One community water system record from CDC My Water's Fluoride.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CdcSystem {
    pub name: String,
    /// PWSID as CDC lists it.
    pub cdc_pwsid: String,
    /// The matching active EPA SDWIS PWSID, when one matched.
    pub sdwis_pwsid: Option<String>,
    pub fluoridated: bool,
    /// Adjusted, Consecutive, Natural, or Non-Adjusted.
    pub status: String,
    pub water_source: String,
    pub population: Option<u64>,
    /// Reported annual average fluoride level (adjusted systems), mg/L.
    pub average_fluoride_mg_l: Option<f64>,
    /// How the record was assigned to the town.
    pub matched_by: String,
    /// Another CDC record covers the same system (excluded from totals).
    pub duplicate: bool,
}

/// A town's fluoridation status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TownFluoridation {
    pub town: String,
    /// From the DPH 2023 list: "fluoridated", "partially fluoridated", "not fluoridated", or
    /// "not fluoridated (no community water system)".
    pub dph_2023_status: String,
    pub dph_2023: Vec<DphRow>,
    /// From CDC My Water's Fluoride systems assigned to the town: "fluoridated",
    /// "partially fluoridated", "naturally occurring fluoride", "not fluoridated",
    /// "no community system listed", or "records disagree".
    pub cdc_status: String,
    pub cdc_report_year: Option<String>,
    pub cdc_systems: Vec<CdcSystem>,
    /// Population on CDC-listed systems that are fluoridated (adjusted, consecutive, or natural).
    pub cdc_population_fluoridated: u64,
    pub cdc_population_total: u64,
    /// True when the DPH 2023 status and the CDC system report point the same way.
    pub sources_agree: bool,
    /// Discrepancies, suspensions, and other notes.
    pub notes: Vec<String>,
}

/// Statewide figures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StatewideFluoridation {
    /// Month of the CDC summary (for example "November 2025").
    pub cdc_period: String,
    pub community_systems: u64,
    pub population_on_community_systems: u64,
    pub fluoridated_systems: u64,
    pub population_fluoridated: u64,
    /// Percent of the population on community systems receiving fluoridated water.
    pub percent_population_fluoridated: f64,
    /// Systems and population by status (Adjusted, Consecutive, Natural, Non-Adjusted, ...).
    pub by_status: Vec<StatusCount>,
    /// Towns by DPH 2023 status.
    pub dph_towns_fully: usize,
    pub dph_towns_partially: usize,
    pub dph_towns_not: usize,
    pub dph_towns_no_community_supply: usize,
}

/// Systems and population in one status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StatusCount {
    /// "Fluoridated" or "Non-fluoridated".
    pub group: String,
    pub status: String,
    pub systems: u64,
    pub population: u64,
}

/// A body's position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PositionOut {
    pub body: String,
    pub kind: String,
    pub date: String,
    pub position: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// An evidence review finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceOut {
    pub review: String,
    pub finding: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// Result of `community_water_fluoridation_status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FluoridationResult {
    pub area: Area,
    pub town: Option<TownFluoridation>,
    pub statewide: StatewideFluoridation,
    pub standards: Vec<CitedOut>,
    pub status_definitions: Vec<CitedOut>,
    /// Each body's current position, side by side (order is not a ranking).
    pub positions: Vec<PositionOut>,
    pub evidence: Vec<EvidenceOut>,
    pub where_they_differ: Vec<CitedOut>,
    pub massachusetts_law: Vec<CitedOut>,
    pub board_of_health_discussion_points: Vec<CitedOut>,
    /// Bodies whose positions could not be fetched and are not shown.
    pub positions_not_included: Vec<String>,
    pub brief_file: Option<String>,
}

// -------------------------------------------------------------------- logic

fn dph_status_of(rows: &[&DphTownStatus]) -> String {
    let has = |s: &str| rows.iter().any(|r| r.status.eq_ignore_ascii_case(s));
    if has("Fully Fluoridated") {
        "fluoridated".into()
    } else if has("Partially Fluoridated") {
        "partially fluoridated".into()
    } else if rows.is_empty() {
        "not listed".into()
    } else if rows.iter().all(|r| {
        r.information
            .to_ascii_lowercase()
            .contains("does not have a community water supply")
    }) {
        "not fluoridated (no community water system)".into()
    } else {
        "not fluoridated".into()
    }
}

/// Status from the CDC system records of a town (duplicates excluded).
pub fn cdc_status_of(systems: &[CdcSystem]) -> String {
    let main: Vec<&CdcSystem> = systems.iter().filter(|s| !s.duplicate).collect();
    if main.is_empty() {
        return "no community system listed".into();
    }
    let pop = |s: &CdcSystem| s.population.unwrap_or(0);
    let total: u64 = main.iter().map(|s| pop(s)).sum();
    let fl: u64 = main.iter().filter(|s| s.fluoridated).map(|s| pop(s)).sum();
    let natural_only = main
        .iter()
        .filter(|s| s.fluoridated)
        .all(|s| s.status.eq_ignore_ascii_case("Natural"));
    if fl == 0 && main.iter().all(|s| !s.fluoridated) {
        "not fluoridated".into()
    } else if natural_only {
        "naturally occurring fluoride".into()
    } else if total > 0 && fl * 10 >= total * 9 {
        // Nine in ten or more of the listed population: the small remainder is usually
        // a mobile home park or condominium with its own well.
        "fluoridated".into()
    } else {
        "partially fluoridated".into()
    }
}

fn agree(dph: &str, cdc: &str) -> bool {
    let d_yes = dph == "fluoridated";
    let d_part = dph == "partially fluoridated";
    let d_no = dph.starts_with("not fluoridated") || dph == "not listed";
    match cdc {
        "fluoridated" => d_yes,
        "partially fluoridated" => d_part || d_yes,
        "naturally occurring fluoride" => true,
        "not fluoridated" | "no community system listed" => d_no,
        _ => false,
    }
}

fn town_status(f: &FluoridationData, town: &str) -> TownFluoridation {
    let dph = f.dph_rows(town);
    let dph_2023_status = dph_status_of(&dph);
    let mut cdc_systems: Vec<CdcSystem> = f
        .mwf_for_town(town)
        .into_iter()
        .map(|s: &MwfSystem| CdcSystem {
            name: s.name.clone(),
            cdc_pwsid: s.mwf_pwsid.clone(),
            sdwis_pwsid: s.sdwis_pwsid.clone(),
            fluoridated: s.fluoridated,
            status: s.status.clone(),
            water_source: s.water_source.clone(),
            population: s.population,
            average_fluoride_mg_l: s.average_fluoride_mg_l,
            matched_by: s.matched_by.clone(),
            duplicate: s.is_duplicate(),
        })
        .collect();
    cdc_systems.sort_by(|a, b| b.population.cmp(&a.population).then(a.name.cmp(&b.name)));
    let mut cdc_status = cdc_status_of(&cdc_systems);
    let mut notes = Vec::new();
    // Records for one system that disagree.
    for d in cdc_systems.iter().filter(|s| s.duplicate) {
        if let Some(main) = cdc_systems
            .iter()
            .find(|m| !m.duplicate && m.sdwis_pwsid.is_some() && m.sdwis_pwsid == d.sdwis_pwsid)
            && main.fluoridated != d.fluoridated
        {
            notes.push(format!(
                "CDC lists {} twice with different answers: {} ({}, {}) and {} ({}, {}). Ask \
                 the water system which is current.",
                main.name,
                main.cdc_pwsid,
                main.status,
                if main.fluoridated {
                    "fluoridated"
                } else {
                    "not fluoridated"
                },
                d.cdc_pwsid,
                d.status,
                if d.fluoridated {
                    "fluoridated"
                } else {
                    "not fluoridated"
                }
            ));
            cdc_status = "records disagree".into();
        }
    }
    for s in cdc_systems.iter().filter(|s| !s.duplicate) {
        if s.status.eq_ignore_ascii_case("Adjusted")
            && s.average_fluoride_mg_l.is_some_and(|v| v <= 0.0)
        {
            notes.push(format!(
                "{} is listed as adjusting fluoride but reported an average of {} mg/L for {}; \
                 fluoridation may be suspended.",
                s.name,
                md::num(s.average_fluoride_mg_l.unwrap_or(0.0), 2),
                f.systems
                    .first()
                    .map(|x| x.report_year.as_str())
                    .unwrap_or("the report year")
            ));
        }
    }
    for r in &dph {
        let info = r.information.trim();
        if !info.is_empty()
            && info != "not applicable"
            && !info.to_ascii_lowercase().starts_with("city/town does")
        {
            notes.push(format!("DPH 2023 note ({}): {info}", r.status));
        }
    }
    let main_pop = |pred: &dyn Fn(&CdcSystem) -> bool| -> u64 {
        cdc_systems
            .iter()
            .filter(|s| !s.duplicate && pred(s))
            .filter_map(|s| s.population)
            .sum()
    };
    let cdc_population_fluoridated = main_pop(&|s| s.fluoridated);
    let cdc_population_total = main_pop(&|_| true);
    let sources_agree = agree(&dph_2023_status, &cdc_status);
    if !sources_agree {
        notes.push(format!(
            "DPH's 2023 list ({dph_2023_status}) and CDC's {} system report ({cdc_status}) differ; \
             the town's water system is the best source for the current status.",
            f.systems
                .first()
                .map(|x| x.report_year.as_str())
                .unwrap_or("")
        ));
    }
    TownFluoridation {
        town: town.to_string(),
        dph_2023_status,
        dph_2023: dph
            .iter()
            .map(|r| DphRow {
                status: r.status.clone(),
                population: r.population,
                year_began: r.year_began.clone(),
                mwra: r.mwra,
                information: r.information.clone(),
            })
            .collect(),
        cdc_status,
        cdc_report_year: f.systems.first().map(|x| x.report_year.clone()),
        cdc_systems,
        cdc_population_fluoridated,
        cdc_population_total,
        sources_agree,
        notes,
    }
}

fn statewide(f: &FluoridationData) -> StatewideFluoridation {
    let row = |g: &str, s: &str| f.summary_row(g, s);
    let total = row("All community water systems", "Total");
    let fl = row("Fluoridated", "Total");
    let pop_total = total.map(|r| r.population).unwrap_or(0);
    let pop_fl = fl.map(|r| r.population).unwrap_or(0);
    let mut fully = 0;
    let mut partially = 0;
    let mut not = 0;
    let mut no_supply = 0;
    let mut towns: Vec<&str> = f.towns.iter().map(|t| t.town.as_str()).collect();
    towns.sort();
    towns.dedup();
    for t in towns {
        let rows = f.dph_rows(t);
        match dph_status_of(&rows).as_str() {
            "fluoridated" => fully += 1,
            "partially fluoridated" => partially += 1,
            "not fluoridated (no community water system)" => {
                not += 1;
                no_supply += 1;
            }
            _ => not += 1,
        }
    }
    StatewideFluoridation {
        cdc_period: f.summary_period.clone(),
        community_systems: total.map(|r| r.systems).unwrap_or(0),
        population_on_community_systems: pop_total,
        fluoridated_systems: fl.map(|r| r.systems).unwrap_or(0),
        population_fluoridated: pop_fl,
        percent_population_fluoridated: if pop_total > 0 {
            (pop_fl as f64 / pop_total as f64 * 1000.0).round() / 10.0
        } else {
            0.0
        },
        by_status: f
            .summary
            .iter()
            .filter(|r| r.group != "All community water systems" && r.status != "Total")
            .filter(|r| r.systems > 0)
            .map(|r| StatusCount {
                group: r.group.clone(),
                status: r.status.clone(),
                systems: r.systems,
                population: r.population,
            })
            .collect(),
        dph_towns_fully: fully,
        dph_towns_partially: partially,
        dph_towns_not: not,
        dph_towns_no_community_supply: no_supply,
    }
}

fn cite_pos(c: &FluoridationContent) -> Vec<PositionOut> {
    c.positions
        .iter()
        .map(|p| PositionOut {
            body: p.body.clone(),
            kind: p.kind.clone(),
            date: p.date.clone(),
            position: p.position.clone(),
            citation: format!(
                "{}, {}",
                c.sources
                    .get(&p.source)
                    .map(|s| s.title.as_str())
                    .unwrap_or(&p.source),
                p.section
            ),
            confidence: p.confidence,
        })
        .collect()
}

fn cite_ev(c: &FluoridationContent) -> Vec<EvidenceOut> {
    c.evidence
        .iter()
        .map(|e| EvidenceOut {
            review: e.review.clone(),
            finding: e.finding.clone(),
            citation: format!(
                "{}, {}",
                c.sources
                    .get(&e.source)
                    .map(|s| s.title.as_str())
                    .unwrap_or(&e.source),
                e.section
            ),
            confidence: e.confidence,
        })
        .collect()
}

/// Fluoridation status, statewide summary, positions, evidence, and law.
pub fn community_water_fluoridation_status(
    ctx: &Context,
    args: FluoridationArgs,
) -> Result<Envelope<FluoridationResult>> {
    let c: FluoridationContent = content::load(FLUORIDATION)?;
    let f = FluoridationData::shared(&ctx.snapshots)?;
    let area = area::resolve(ctx, args.town.as_deref(), true)?;
    if area.level == AreaLevel::County {
        return Err(phn_core::PhnError::InvalidInput(format!(
            "{} is a county; fluoridation is decided town by town. Name a city or town, or \
             \"Massachusetts\" for the statewide summary.",
            area.name
        )));
    }
    let town = (area.level == AreaLevel::Town).then(|| town_status(f, &area.name));
    let sw = statewide(f);
    let cite = |x: &[Cited]| content::cite_all(x, &c.sources);
    let positions = cite_pos(&c);
    let evidence = cite_ev(&c);
    let standards = cite(&c.standards);
    let status_definitions = cite(&c.status_definitions);
    let where_they_differ = cite(&c.divergences);
    let massachusetts_law = cite(&c.ma_law);
    let discussion = cite(&c.discussion_points);

    // Summary.
    let mut s = format!("**Community water fluoridation — {}**\n\n", area.name);
    if let Some(t) = &town {
        s.push_str(&format!(
            "- **DPH 2023 list:** {}{}\n- **CDC My Water's Fluoride ({} report):** {}{}\n",
            t.dph_2023_status,
            t.dph_2023
                .iter()
                .filter_map(|r| r.year_began.as_ref().map(|y| format!(" (since {y})")))
                .next()
                .unwrap_or_default(),
            t.cdc_report_year.clone().unwrap_or_default(),
            t.cdc_status,
            if t.cdc_population_total > 0 {
                format!(
                    " — {} of {} people on listed systems receive fluoridated water",
                    md::num(t.cdc_population_fluoridated as f64, 0),
                    md::num(t.cdc_population_total as f64, 0)
                )
            } else {
                String::new()
            }
        ));
        if t.dph_2023.iter().any(|r| r.mwra) {
            s.push_str("- Receives fluoridated water from the MWRA (DPH).\n");
        }
        if !t.cdc_systems.is_empty() {
            s.push('\n');
            let rows: Vec<Vec<String>> = t
                .cdc_systems
                .iter()
                .map(|x| {
                    vec![
                        format!(
                            "{}{}",
                            x.name,
                            if x.duplicate {
                                " (duplicate record)"
                            } else {
                                ""
                            }
                        ),
                        x.sdwis_pwsid
                            .clone()
                            .unwrap_or_else(|| x.cdc_pwsid.replace('-', "")),
                        x.status.clone(),
                        if x.fluoridated { "yes" } else { "no" }.into(),
                        x.population
                            .map(|p| md::num(p as f64, 0))
                            .unwrap_or_else(|| "—".into()),
                        x.average_fluoride_mg_l
                            .map(|v| format!("{} mg/L", md::num(v, 2)))
                            .unwrap_or_else(|| "—".into()),
                    ]
                })
                .collect();
            s.push_str(&md::table(
                &[
                    "System (CDC)",
                    "PWSID",
                    "Status",
                    "Fluoridated",
                    "Population",
                    "Avg fluoride",
                ],
                &rows,
            ));
        }
        for n in &t.notes {
            s.push_str(&format!("\n- {n}"));
        }
        s.push_str("\n\n");
    }
    s.push_str(&format!(
        "**Statewide (CDC, {}):** {} of {} people on community water systems ({}%) receive \
         fluoridated water ({} of {} systems). DPH 2023 list: {} towns fully fluoridated, {} \
         partially, {} not fluoridated (DPH notes {} of these have no community water supply).\n\n",
        sw.cdc_period,
        md::num(sw.population_fluoridated as f64, 0),
        md::num(sw.population_on_community_systems as f64, 0),
        md::num(sw.percent_population_fluoridated, 1),
        sw.fluoridated_systems,
        sw.community_systems,
        sw.dph_towns_fully,
        sw.dph_towns_partially,
        sw.dph_towns_not,
        sw.dph_towns_no_community_supply
    ));
    s.push_str("**Positions (side by side; dates matter — this area changed in 2025–2026):**\n\n");
    s.push_str(&md::table(
        &["Body", "Date", "Position"],
        &positions
            .iter()
            .map(|p| {
                vec![
                    format!(
                        "{}{}",
                        p.body,
                        if p.confidence == Confidence::High {
                            ""
                        } else {
                            " (medium confidence)"
                        }
                    ),
                    p.date.clone(),
                    p.position.clone(),
                ]
            })
            .collect::<Vec<_>>(),
    ));
    s.push_str("\n**Evidence reviews:**\n");
    for e in &evidence {
        s.push_str(&format!("- **{}:** {}\n", e.review, e.finding));
    }
    s.push_str("\n**Where they differ:**\n");
    for d in &where_they_differ {
        s.push_str(&format!("- {}\n", d.text));
    }
    s.push_str("\n**Massachusetts law (M.G.L. c. 111, § 8C):** the board of health may order fluoridation after inquiry if in the inhabitants' best interest; the order takes effect 90 days after newspaper publication; 10% of registered voters can petition within 90 days for a ballot question; a no vote bars a new order for two years.\n");
    s.push_str(&format!(
        "\nBoard of health discussion points ({}) are in the data{}.\n",
        discussion.len(),
        if args.brief.unwrap_or(false) {
            " and the brief"
        } else {
            "; ask for brief = true to write them to a DOCX"
        }
    ));

    // Brief.
    let mut files_written = Vec::new();
    let mut brief_file = None;
    if args.brief.unwrap_or(false) {
        let doc = brief(
            ctx,
            &c,
            &area,
            town.as_ref(),
            &sw,
            &positions,
            &evidence,
            &standards,
            &where_they_differ,
            &massachusetts_law,
            &discussion,
        );
        let bytes = phn_docs::docx::to_docx(&doc)?;
        let sandbox = ctx.sandbox()?;
        let slug: String = area
            .name
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        let path = sandbox.new_output_file(&format!("fluoridation_brief_{slug}.docx"))?;
        std::fs::write(&path, bytes)?;
        let disp = sandbox.display_path(&path);
        files_written.push(disp.clone());
        brief_file = Some(disp);
    }

    let mut caveats = vec![
        "DPH's town list reflects 2023; DPH notes that some fluoridated communities may have \
         temporarily suspended fluoridation. CDC's system report is state-reported for 2025 and \
         may show targets rather than test results. The town's water system is the best source \
         for the current fluoride level."
            .to_string(),
        "Systems are assigned to towns by this project (matched to EPA SDWIS by PWSID or name); \
         a system can serve parts of other towns. Some CDC records carry outdated PWSIDs or \
         duplicate a system; duplicates are shown but not counted."
            .to_string(),
        "Positions are shown side by side with their dates and are not ranked; the federal \
         picture changed in 2025–2026 (FDA October 2025; EPA review 2026). Re-check each source \
         before a vote."
            .to_string(),
        format!(
            "Not included because their sites could not be reached: {}.",
            c.not_included.join("; ")
        ),
    ];
    caveats.extend(content::review_caveats(&[FLUORIDATION]));
    let mut sources: Vec<Source> = f.sources();
    sources.extend(content::sources_for(
        c.sources.keys().map(String::as_str),
        &c.sources,
    ));

    Ok(Envelope::new(
        s,
        FluoridationResult {
            area,
            town,
            statewide: sw,
            standards,
            status_definitions,
            positions,
            evidence,
            where_they_differ,
            massachusetts_law,
            board_of_health_discussion_points: discussion,
            positions_not_included: c.not_included.clone(),
            brief_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        files_written,
        ..Privacy::none()
    })
    .decision_support())
}

#[allow(clippy::too_many_arguments)]
fn brief(
    ctx: &Context,
    c: &FluoridationContent,
    area: &Area,
    town: Option<&TownFluoridation>,
    sw: &StatewideFluoridation,
    positions: &[PositionOut],
    evidence: &[EvidenceOut],
    standards: &[CitedOut],
    differ: &[CitedOut],
    law: &[CitedOut],
    discussion: &[CitedOut],
) -> Document {
    let p = |t: &str| Block::Paragraph(vec![Inline::plain(t)]);
    let h = |t: &str| Block::Heading {
        level: 2,
        text: t.to_string(),
    };
    let list = |items: Vec<String>| Block::List {
        ordered: false,
        items: items.into_iter().map(|i| vec![Inline::plain(i)]).collect(),
    };
    let mut blocks = vec![p(&format!(
        "Prepared {} for {}. Decision support only: positions are shown side by side with their \
         dates and are not ranked. Content review status: pending subject matter expert review. \
         Re-check each source before a board of health vote.",
        chrono::Local::now().date_naive(),
        area.name
    ))];
    blocks.push(h("1. Status"));
    if let Some(t) = town {
        blocks.push(p(&format!(
            "DPH 2023 list: {}. CDC My Water's Fluoride ({} report): {}.",
            t.dph_2023_status,
            t.cdc_report_year.clone().unwrap_or_default(),
            t.cdc_status
        )));
        if !t.cdc_systems.is_empty() {
            blocks.push(Block::Table(TableBlock {
                caption: Some(format!("Community water systems in {} (CDC)", t.town)),
                headers: vec![
                    "System".into(),
                    "PWSID".into(),
                    "Status".into(),
                    "Fluoridated".into(),
                    "Population".into(),
                    "Average fluoride (mg/L)".into(),
                ],
                rows: t
                    .cdc_systems
                    .iter()
                    .map(|x| {
                        vec![
                            format!(
                                "{}{}",
                                x.name,
                                if x.duplicate {
                                    " (duplicate record)"
                                } else {
                                    ""
                                }
                            ),
                            x.sdwis_pwsid.clone().unwrap_or_else(|| x.cdc_pwsid.clone()),
                            x.status.clone(),
                            if x.fluoridated { "Yes" } else { "No" }.into(),
                            x.population.map(|v| v.to_string()).unwrap_or_default(),
                            x.average_fluoride_mg_l
                                .map(|v| md::num(v, 2))
                                .unwrap_or_default(),
                        ]
                    })
                    .collect(),
            }));
        }
        if !t.notes.is_empty() {
            blocks.push(list(t.notes.clone()));
        }
    }
    blocks.push(p(&format!(
        "Statewide (CDC, {}): {}% of people on community water systems receive fluoridated water \
         ({} of {} people). DPH 2023: {} towns fully fluoridated, {} partially, {} not \
         fluoridated.",
        sw.cdc_period,
        md::num(sw.percent_population_fluoridated, 1),
        md::num(sw.population_fluoridated as f64, 0),
        md::num(sw.population_on_community_systems as f64, 0),
        sw.dph_towns_fully,
        sw.dph_towns_partially,
        sw.dph_towns_not
    )));
    blocks.push(h("2. Standards"));
    blocks.push(list(
        standards
            .iter()
            .map(|x| format!("{} ({})", x.text, x.citation))
            .collect(),
    ));
    blocks.push(h("3. Positions, side by side"));
    blocks.push(Block::Table(TableBlock {
        caption: Some("Current positions (order is not a ranking)".into()),
        headers: vec![
            "Body".into(),
            "Date".into(),
            "Position".into(),
            "Source".into(),
        ],
        rows: positions
            .iter()
            .map(|x| {
                vec![
                    x.body.clone(),
                    x.date.clone(),
                    x.position.clone(),
                    x.citation.clone(),
                ]
            })
            .collect(),
    }));
    blocks.push(p(&format!(
        "Not included (could not be reached): {}.",
        c.not_included.join("; ")
    )));
    blocks.push(h("4. Evidence reviews"));
    blocks.push(Block::Table(TableBlock {
        caption: Some("Evidence reviews".into()),
        headers: vec!["Review".into(), "Finding".into(), "Source".into()],
        rows: evidence
            .iter()
            .map(|x| vec![x.review.clone(), x.finding.clone(), x.citation.clone()])
            .collect(),
    }));
    blocks.push(h("5. Where they differ"));
    blocks.push(list(differ.iter().map(|x| x.text.clone()).collect()));
    blocks.push(h("6. Massachusetts law: M.G.L. c. 111, § 8C"));
    blocks.push(list(law.iter().map(|x| x.text.clone()).collect()));
    blocks.push(h("7. Discussion points for the board of health (draft)"));
    blocks.push(list(discussion.iter().map(|x| x.text.clone()).collect()));
    blocks.push(h("8. Sources"));
    blocks.push(list(
        c.sources
            .values()
            .map(|s| {
                format!(
                    "{} — {} ({}){}; retrieved {}",
                    s.title,
                    s.publisher,
                    s.url,
                    s.effective_date
                        .as_deref()
                        .map(|d| format!(", dated {d}"))
                        .unwrap_or_default(),
                    s.retrieved
                )
            })
            .chain([
                "CDC My Water's Fluoride, Massachusetts Fluoridation Status Report and Monthly Summary Report (nccd.cdc.gov/doh_mwf), retrieved 2026-11-11".to_string(),
                "DPH 2023 List of Massachusetts Cities/Towns by Community Water Fluoridation Status (mass.gov), retrieved 2026-11-11".to_string(),
            ])
            .collect(),
    ));
    Document {
        title: format!("Community water fluoridation — {}", area.name),
        subtitle: Some("Briefing for a board of health discussion".into()),
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    fn sys(pop: u64, fl: bool, status: &str, dup: bool) -> CdcSystem {
        CdcSystem {
            name: "X".into(),
            cdc_pwsid: "MA-0".into(),
            sdwis_pwsid: None,
            fluoridated: fl,
            status: status.into(),
            water_source: "Ground".into(),
            population: Some(pop),
            average_fluoride_mg_l: None,
            matched_by: String::new(),
            duplicate: dup,
        }
    }

    #[test]
    fn content_loads_and_every_citation_resolves() {
        let c: FluoridationContent = content::load(FLUORIDATION).unwrap();
        for k in c
            .standards
            .iter()
            .chain(&c.status_definitions)
            .chain(&c.divergences)
            .chain(&c.ma_law)
            .chain(&c.discussion_points)
            .map(|x| &x.source)
            .chain(c.positions.iter().map(|p| &p.source))
            .chain(c.evidence.iter().map(|e| &e.source))
        {
            assert!(c.sources.contains_key(k), "{k}");
        }
        // The draft discussion points are never presented as high confidence.
        assert!(
            c.discussion_points
                .iter()
                .all(|d| d.confidence != Confidence::High)
        );
        assert!(c.positions.len() >= 5);
    }

    #[test]
    fn cdc_status_rules() {
        assert_eq!(cdc_status_of(&[]), "no community system listed");
        assert_eq!(
            cdc_status_of(&[sys(1000, true, "Adjusted", false)]),
            "fluoridated"
        );
        assert_eq!(
            cdc_status_of(&[sys(1000, false, "Non-Adjusted", false)]),
            "not fluoridated"
        );
        assert_eq!(
            cdc_status_of(&[
                sys(1000, true, "Consecutive", false),
                sys(1000, false, "Non-Adjusted", false)
            ]),
            "partially fluoridated"
        );
        // A small non-fluoridated park beside a large fluoridated system.
        assert_eq!(
            cdc_status_of(&[
                sys(20000, true, "Adjusted", false),
                sys(80, false, "Non-Adjusted", false)
            ]),
            "fluoridated"
        );
        assert_eq!(
            cdc_status_of(&[
                sys(100, true, "Natural", false),
                sys(5000, false, "Non-Adjusted", false)
            ]),
            "naturally occurring fluoride"
        );
        // Duplicates are not counted.
        assert_eq!(
            cdc_status_of(&[
                sys(1000, false, "Non-Adjusted", false),
                sys(1000, true, "Adjusted", true)
            ]),
            "not fluoridated"
        );
    }

    #[test]
    fn towns_and_statewide_summary() {
        let (_d, ctx) = ctx();
        let env = community_water_fluoridation_status(
            &ctx,
            FluoridationArgs {
                town: Some("Amherst".into()),
                brief: None,
            },
        )
        .unwrap();
        let t = env.data.town.as_ref().unwrap();
        assert_eq!(t.dph_2023_status, "fluoridated");
        assert_eq!(t.cdc_status, "fluoridated");
        assert!(t.sources_agree);
        assert!(env.notice.is_some());
        assert!(env.data.statewide.percent_population_fluoridated > 50.0);
        assert!(env.summary.contains("Positions"));
        assert_eq!(
            env.data.statewide.dph_towns_fully
                + env.data.statewide.dph_towns_partially
                + env.data.statewide.dph_towns_not,
            351
        );

        // Pelham: partly served by Amherst.
        let env = community_water_fluoridation_status(
            &ctx,
            FluoridationArgs {
                town: Some("Pelham".into()),
                brief: None,
            },
        )
        .unwrap();
        assert_eq!(
            env.data.town.as_ref().unwrap().dph_2023_status,
            "partially fluoridated"
        );

        // Fall River: listed as adjusted but averaging 0.00 mg/L.
        let env = community_water_fluoridation_status(
            &ctx,
            FluoridationArgs {
                town: Some("Fall River".into()),
                brief: None,
            },
        )
        .unwrap();
        let t = env.data.town.as_ref().unwrap();
        assert!(
            t.notes.iter().any(|n| n.contains("may be suspended")),
            "{:?}",
            t.notes
        );

        // Statewide only.
        let env = community_water_fluoridation_status(
            &ctx,
            FluoridationArgs {
                town: Some("Massachusetts".into()),
                brief: None,
            },
        )
        .unwrap();
        assert!(env.data.town.is_none());
        // Counties are a tool error.
        assert!(
            community_water_fluoridation_status(
                &ctx,
                FluoridationArgs {
                    town: Some("Hampshire County".into()),
                    brief: None,
                }
            )
            .is_err()
        );
    }

    #[test]
    fn brief_is_written_to_output() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(d.path())).unwrap();
        let env = community_water_fluoridation_status(
            &ctx,
            FluoridationArgs {
                town: Some("Hingham".into()),
                brief: Some(true),
            },
        )
        .unwrap();
        let f = env.data.brief_file.clone().unwrap();
        assert!(f.ends_with(".docx"), "{f}");
        assert_eq!(env.privacy.files_written, vec![f.clone()]);
        let on_disk = std::fs::read_dir(d.path().join("data/output"))
            .unwrap()
            .count();
        assert_eq!(on_disk, 1);
        // Hingham's two CDC records for one system disagree.
        let t = env.data.town.as_ref().unwrap();
        assert!(t.notes.iter().any(|n| n.contains("twice")), "{:?}", t.notes);
    }
}

#[cfg(test)]
mod print_tests {
    use super::*;

    /// `cargo test -p phn-tools --lib fluoridation::print_tests -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn print_summaries() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        for town in ["Hingham", "Worcester", "Charlton"] {
            let env = community_water_fluoridation_status(
                &ctx,
                FluoridationArgs {
                    town: Some(town.into()),
                    brief: None,
                },
            )
            .unwrap();
            eprintln!("{}\n-----", env.summary);
        }
    }
}
