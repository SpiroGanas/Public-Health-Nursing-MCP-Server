//! EPR-11 `plan_casper_assessment`: plan a CDC CASPER (two-stage cluster
//! sample of census blocks selected with probability proportional to
//! households, seven interviews per cluster), produce team materials, and
//! analyze returned data with the toolkit's weights and design-based
//! confidence intervals (aggregates only).

use std::collections::{BTreeMap, BTreeSet};

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::census_blocks::{Block, BlockFrame};
use phn_data::geography::MaGeographies;
use phn_docs::model::{Block as DocBlock, Document, Inline, TableBlock};
use phn_epi::survey::{self, casper_weight};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::area::{self, Area, AreaLevel};
use super::content::{self, Casper, CitedOut, NamedFormula};
use crate::context::Context;
use crate::md;

/// Tracking-form totals for response rates.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct TrackingTotals {
    /// Households where contact was attempted (completed, incomplete, refusals, no answer, vacant, inaccessible).
    pub attempted: Option<u32>,
    /// Households where contact was made (completed, incomplete, refusals).
    pub contacted: Option<u32>,
    /// Completed interviews.
    pub completed: Option<u32>,
}

/// Arguments for `plan_casper_assessment`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CasperArgs {
    /// The sampling frame: a Massachusetts city or town, or a county (default: your saved town).
    pub area: Option<String>,
    /// Or several towns combined into one frame.
    pub towns: Vec<String>,
    /// Clusters to select (default 30; any other number is a modified CASPER).
    pub clusters: Option<u32>,
    /// Interviews per cluster (default 7).
    pub interviews_per_cluster: Option<u32>,
    /// "total" housing units (default) or "occupied" households.
    pub household_measure: Option<String>,
    /// Random seed for a reproducible selection (default: chosen and reported).
    pub seed: Option<u64>,
    /// Leave out blocks with fewer households than this (a modified frame).
    pub exclude_blocks_under: Option<u32>,
    /// Write the cluster list, tracking forms, and team packet to /data/output (default true).
    pub materials: Option<bool>,
    /// Analysis: a CSV/XLSX in /data with one row per completed interview.
    pub results_path: Option<String>,
    /// Analysis: the cluster-number column (default "cluster").
    pub cluster_column: Option<String>,
    /// Analysis: column giving times each cluster was selected (default 1).
    pub times_selected_column: Option<String>,
    /// Analysis: questions (columns) to estimate, e.g. ["damage", "has_generator"].
    pub questions: Vec<String>,
    /// Analysis: households in the sampling frame (default: recomputed from the area and measure).
    pub frame_households: Option<f64>,
    /// Analysis: tracking-form totals for contact and cooperation rates.
    pub tracking: Option<TrackingTotals>,
}

/// A selected cluster.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Cluster {
    /// Cluster number (1, 2, ...), in selection order of first draw.
    pub cluster: u32,
    /// 15-digit census block GEOID.
    pub block_geoid: String,
    pub town: String,
    pub tract: String,
    pub block_group: String,
    pub block: String,
    /// Households used for selection (the chosen measure).
    pub households: u32,
    pub housing_units: u32,
    pub vacant: u32,
    pub times_selected: u32,
    pub interviews_to_complete: u32,
    /// Stage II sampling interval n = households ÷ 7 (rounded, at least 1).
    pub sampling_interval: u32,
}

/// One category's estimate (cells with fewer than 5 observations are suppressed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CasperEstimate {
    pub category: String,
    /// Unweighted count ("<5" when fewer than 5).
    pub frequency: String,
    pub estimated_households: Option<f64>,
    pub percent: Option<f64>,
    pub ci_lower: Option<f64>,
    pub ci_upper: Option<f64>,
    pub design_effect: Option<f64>,
}

/// Estimates for one question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuestionResult {
    pub question: String,
    pub respondents: usize,
    pub clusters: usize,
    /// True when weighted (completion rate ≥ 80%); otherwise unweighted percentages.
    pub weighted: bool,
    pub estimates: Vec<CasperEstimate>,
}

/// Analysis of returned data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CasperAnalysisResult {
    pub file: String,
    pub completed_interviews: usize,
    pub clusters_with_interviews: usize,
    pub frame_households: f64,
    pub clusters_selected: u32,
    pub completion_rate: f64,
    pub contact_rate: Option<f64>,
    pub cooperation_rate: Option<f64>,
    pub weighted: bool,
    pub questions: Vec<QuestionResult>,
}

/// Result of `plan_casper_assessment`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CasperResult {
    pub frame: String,
    pub household_measure: String,
    pub frame_blocks: usize,
    pub frame_households: u64,
    /// Share of frame blocks with fewer households than the interviews per cluster.
    pub small_block_share: f64,
    pub seed: u64,
    pub design: String,
    pub modified: bool,
    pub clusters: Vec<Cluster>,
    pub selection_method: String,
    pub stage2_steps: Vec<String>,
    pub weight_formula: String,
    pub response_rates: Vec<NamedFormula>,
    pub consent_elements: Vec<String>,
    pub questionnaire_guidance: Vec<CitedOut>,
    pub team_guidance: Vec<CitedOut>,
    pub supplies: Vec<String>,
    pub analysis: Option<CasperAnalysisResult>,
    pub files: Vec<String>,
}

/// SplitMix64 (Steele, Lea, and Flood, 2014): a small, fully specified
/// generator, so the same seed always selects the same clusters.
#[derive(Debug, Clone)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform integer in 1..=n (rejection sampling, no modulo bias).
    pub fn between_1_and(&mut self, n: u64) -> u64 {
        let zone = u64::MAX - (u64::MAX % n);
        loop {
            let r = self.next_u64();
            if r < zone {
                return r % n + 1;
            }
        }
    }
}

/// Stage I: draw `k` random household numbers in 1..=total and return the
/// index of the block containing each (cumulative sums), in draw order.
pub fn select_pps(sizes: &[u32], k: u32, rng: &mut SplitMix64) -> Vec<usize> {
    let mut cum = Vec::with_capacity(sizes.len());
    let mut total = 0u64;
    for &s in sizes {
        total += s as u64;
        cum.push(total);
    }
    (0..k)
        .map(|_| {
            let r = rng.between_1_and(total);
            cum.partition_point(|&c| c < r)
        })
        .collect()
}

fn frame_for(ctx: &Context, args: &CasperArgs) -> Result<(String, Vec<String>)> {
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    if !args.towns.is_empty() {
        let mut ids = Vec::new();
        let mut names = Vec::new();
        for t in &args.towns {
            let m = ctx.ma.town(t).ok_or_else(|| {
                PhnError::NotFound(format!("'{t}' is not a Massachusetts city or town"))
            })?;
            let g = geos
                .town(&m.name)
                .ok_or_else(|| PhnError::Internal(format!("no GEOID for {}", m.name)))?;
            ids.push(g.geoid.clone());
            names.push(m.name.clone());
        }
        return Ok((names.join(", "), ids));
    }
    let a: Area = area::resolve(ctx, args.area.as_deref(), false)?;
    match a.level {
        AreaLevel::State => Err(PhnError::InvalidInput(
            "a CASPER frame is a town, several towns, or a county, not the whole state".into(),
        )),
        AreaLevel::County => Ok((a.name.clone(), vec![a.geoid.clone()])),
        AreaLevel::Town => Ok((a.name.clone(), vec![a.geoid.clone()])),
    }
}

fn in_frame(b: &Block, ids: &[String]) -> bool {
    ids.iter().any(|id| match id.len() {
        5 => b.county_geoid() == *id,
        _ => b.town_geoid() == *id,
    })
}

/// Plan a CASPER and (optionally) analyze returned data.
pub fn plan_casper_assessment(ctx: &Context, args: CasperArgs) -> Result<Envelope<CasperResult>> {
    let c: Casper = content::load(content::CASPER)?;
    let k = args.clusters.unwrap_or(c.design.clusters);
    let per = args
        .interviews_per_cluster
        .unwrap_or(c.design.interviews_per_cluster);
    if !(2..=200).contains(&k) {
        return Err(PhnError::InvalidInput("clusters must be 2-200".into()));
    }
    if !(1..=30).contains(&per) {
        return Err(PhnError::InvalidInput(
            "interviews_per_cluster must be 1-30".into(),
        ));
    }
    let measure = args
        .household_measure
        .as_deref()
        .unwrap_or("total")
        .trim()
        .to_ascii_lowercase();
    let occupied = match measure.as_str() {
        "total" | "housing units" | "all" => false,
        "occupied" | "households" | "occupied households" => true,
        other => {
            return Err(PhnError::InvalidInput(format!(
                "household_measure must be total or occupied (got '{other}')"
            )));
        }
    };
    let (frame_name, ids) = frame_for(ctx, &args)?;
    let blocks = BlockFrame::shared(&ctx.snapshots)?;
    let size = |b: &Block| {
        if occupied {
            b.occupied()
        } else {
            b.housing_units
        }
    };
    let min = args.exclude_blocks_under.unwrap_or(1).max(1);
    let frame: Vec<&Block> = blocks
        .blocks
        .iter()
        .filter(|b| in_frame(b, &ids) && size(b) >= min)
        .collect();
    if frame.len() < k as usize {
        return Err(PhnError::InvalidInput(format!(
            "{frame_name} has only {} census blocks with households; a cluster design needs more \
             blocks than clusters. The toolkit suggests a different method (it would not be a \
             CASPER) when the frame is this small.",
            frame.len()
        )));
    }
    let sizes: Vec<u32> = frame.iter().map(|b| size(b)).collect();
    let frame_households: u64 = sizes.iter().map(|&s| s as u64).sum();
    let small = sizes.iter().filter(|&&s| s < per).count() as f64 / sizes.len() as f64;
    let seed = args.seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(1)
    });
    let mut rng = SplitMix64::new(seed);
    let draws = select_pps(&sizes, k, &mut rng);
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let mut order: Vec<usize> = Vec::new();
    let mut times: BTreeMap<usize, u32> = BTreeMap::new();
    for d in &draws {
        if !order.contains(d) {
            order.push(*d);
        }
        *times.entry(*d).or_default() += 1;
    }
    let clusters: Vec<Cluster> = order
        .iter()
        .enumerate()
        .map(|(i, &bi)| {
            let b = frame[bi];
            let t = times[&bi];
            let hh = size(b);
            Cluster {
                cluster: i as u32 + 1,
                block_geoid: b.geoid(),
                town: geos
                    .get(&b.town_geoid())
                    .map(|g| g.name.clone())
                    .unwrap_or_else(|| b.town_geoid()),
                tract: format!("{:06}", b.tract),
                block_group: (b.block / 1000).to_string(),
                block: format!("{:04}", b.block),
                households: hh,
                housing_units: b.housing_units,
                vacant: b.vacant,
                times_selected: t,
                interviews_to_complete: per * t,
                sampling_interval: ((hh as f64 / per as f64).round() as u32).max(1),
            }
        })
        .collect();
    let modified = k != c.design.clusters || per != c.design.interviews_per_cluster || min > 1;
    let mut caveats = Vec::new();
    if modified {
        caveats.push(format!(
            "This is a MODIFIED CASPER ({}): describe it as modified in the methods and every report.",
            [
                (k != c.design.clusters).then(|| format!("{k} clusters")),
                (per != c.design.interviews_per_cluster).then(|| format!("{per} interviews per cluster")),
                (min > 1).then(|| format!("blocks with fewer than {min} households excluded from the frame")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("; ")
        ));
    }
    let small_selected = clusters
        .iter()
        .filter(|cl| cl.households < cl.interviews_to_complete)
        .count();
    if small_selected > 0 {
        caveats.push(format!(
            "{small_selected} selected cluster(s) have fewer households than interviews needed; \
             {:.0}% of frame blocks have fewer than {per}. The toolkit suggests checking this \
             before selecting and, if common, using block groups or joining adjacent blocks.",
            small * 100.0
        ));
    }
    let repeated = clusters.iter().filter(|cl| cl.times_selected > 1).count();

    // Analysis.
    let mut analysis = None;
    let mut identifiers = Vec::new();
    let mut local_src = None;
    if let Some(p) = &args.results_path {
        let lt = crate::localdata::load(ctx, p, None)?;
        identifiers = lt.identifier_categories();
        let ccol = lt.column(args.cluster_column.as_deref().unwrap_or("cluster"))?;
        let tcol = match &args.times_selected_column {
            Some(n) => Some(lt.column(n)?),
            None => None,
        };
        let mut cluster_ids: Vec<usize> = Vec::new();
        let mut keys: Vec<String> = Vec::new();
        let mut times_by: BTreeMap<String, f64> = BTreeMap::new();
        for row in &lt.table.rows {
            let key = row
                .get(ccol)
                .map(|v| v.trim().to_string())
                .unwrap_or_default();
            if key.is_empty() {
                return Err(PhnError::InvalidInput(
                    "every row needs a cluster number".into(),
                ));
            }
            if let Some(tc) = tcol {
                let t: f64 = row
                    .get(tc)
                    .and_then(|v| v.trim().parse().ok())
                    .filter(|t: &f64| *t >= 1.0)
                    .ok_or_else(|| {
                        PhnError::InvalidInput("times selected must be 1 or more".into())
                    })?;
                times_by.insert(key.clone(), t);
            }
            let idx = match keys.iter().position(|k| k == &key) {
                Some(i) => i,
                None => {
                    keys.push(key.clone());
                    keys.len() - 1
                }
            };
            cluster_ids.push(idx);
        }
        let n = cluster_ids.len();
        let mut per_cluster = vec![0usize; keys.len()];
        for &i in &cluster_ids {
            per_cluster[i] += 1;
        }
        let n_frame = args.frame_households.unwrap_or(frame_households as f64);
        if !(n_frame.is_finite() && n_frame > 0.0) {
            return Err(PhnError::InvalidInput(
                "frame_households must be positive".into(),
            ));
        }
        let weights: Vec<f64> = cluster_ids
            .iter()
            .map(|&i| {
                let t = times_by.get(&keys[i]).copied().unwrap_or(1.0);
                casper_weight(n_frame, per_cluster[i] as f64 / t, k as f64)
            })
            .collect();
        let intended = (k * per) as f64;
        let completion = n as f64 / intended;
        let weighted = completion >= c.design.minimum_completion_rate;
        if !weighted {
            caveats.push(format!(
                "Completion rate {:.1}% is below 80% ({} of {intended} interviews): the toolkit \
                 calls for returning to the field or presenting UNWEIGHTED results, so only \
                 unweighted percentages are shown.",
                completion * 100.0,
                n
            ));
        }
        let rates = args.tracking.clone().unwrap_or_default();
        let completed_for_rates = rates.completed.map(|c| c as f64).unwrap_or(n as f64);
        let contact_rate = rates
            .attempted
            .filter(|a| *a > 0)
            .map(|a| (completed_for_rates / a as f64 * 1000.0).round() / 10.0);
        let cooperation_rate = rates
            .contacted
            .filter(|a| *a > 0)
            .map(|a| (completed_for_rates / a as f64 * 1000.0).round() / 10.0);
        let min_cell = c.design.minimum_cell_observations;
        let mut questions = Vec::new();
        for q in &args.questions {
            let qi = lt.column(q)?;
            let vals = lt.categorical(qi)?;
            let labels: Vec<String> = vals
                .iter()
                .flatten()
                .cloned()
                .collect::<BTreeSet<String>>()
                .into_iter()
                .collect();
            let answers: Vec<Option<usize>> = vals
                .iter()
                .map(|v| v.as_ref().and_then(|v| labels.iter().position(|l| l == v)))
                .collect();
            let w: Vec<f64> = if weighted {
                weights.clone()
            } else {
                vec![1.0; n]
            };
            let t = survey::weighted_proportions(&cluster_ids, &w, &answers, &labels, 0.95)
                .ok_or_else(|| {
                    PhnError::InvalidInput(format!(
                        "'{q}' cannot be estimated: answers are needed from at least two clusters"
                    ))
                })?;
            let estimates = t
                .categories
                .iter()
                .map(|e| {
                    let show = e.n >= min_cell;
                    let r1 = |x: f64| (x * 10.0).round() / 10.0;
                    CasperEstimate {
                        category: e.category.clone(),
                        frequency: if e.n > 0 && e.n < min_cell {
                            format!("<{min_cell}")
                        } else {
                            e.n.to_string()
                        },
                        estimated_households: (show && weighted).then(|| e.weighted_count.round()),
                        percent: show.then(|| r1(e.percent)),
                        ci_lower: (show && weighted).then(|| r1(e.ci_lower)),
                        ci_upper: (show && weighted).then(|| r1(e.ci_upper)),
                        design_effect: if show && weighted {
                            e.design_effect.map(|d| (d * 100.0).round() / 100.0)
                        } else {
                            None
                        },
                    }
                })
                .collect();
            questions.push(QuestionResult {
                question: lt.table.headers[qi].clone(),
                respondents: t.n,
                clusters: t.clusters,
                weighted,
                estimates,
            });
        }
        analysis = Some(CasperAnalysisResult {
            file: lt.label.clone(),
            completed_interviews: n,
            clusters_with_interviews: keys.len(),
            frame_households: n_frame,
            clusters_selected: k,
            completion_rate: (completion * 1000.0).round() / 10.0,
            contact_rate,
            cooperation_rate,
            weighted,
            questions,
        });
        local_src = Some(lt.source());
    }

    // Materials.
    let mut files = Vec::new();
    if args.materials.unwrap_or(true) {
        let sandbox = ctx.sandbox()?;
        let crows: Vec<Vec<String>> = clusters
            .iter()
            .map(|cl| {
                vec![
                    cl.cluster.to_string(),
                    cl.block_geoid.clone(),
                    cl.town.clone(),
                    cl.tract.clone(),
                    cl.block_group.clone(),
                    cl.block.clone(),
                    cl.households.to_string(),
                    cl.times_selected.to_string(),
                    cl.interviews_to_complete.to_string(),
                    cl.sampling_interval.to_string(),
                    String::new(),
                ]
            })
            .collect();
        let mut track_rows = Vec::new();
        for tc in &c.tracking_categories {
            for o in &tc.outcomes {
                track_rows.push(vec![
                    tc.category.clone(),
                    o.clone(),
                    String::new(),
                    String::new(),
                ]);
            }
        }
        let xdoc = Document {
            title: format!("CASPER cluster list — {frame_name}"),
            subtitle: Some(format!(
                "Seed {seed}; {} measure",
                if occupied {
                    "occupied households"
                } else {
                    "total housing units"
                }
            )),
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks: vec![
                DocBlock::Table(TableBlock {
                    caption: Some("Selected clusters".into()),
                    headers: [
                        "Cluster",
                        "Census block GEOID",
                        "Town",
                        "Tract",
                        "Block group",
                        "Block",
                        "Households",
                        "Times selected",
                        "Interviews to complete",
                        "Sampling interval n",
                        "Team",
                    ]
                    .map(String::from)
                    .to_vec(),
                    rows: crows,
                }),
                DocBlock::Table(TableBlock {
                    caption: Some("Tracking form (one per cluster)".into()),
                    headers: vec![
                        "Category".into(),
                        "Outcome".into(),
                        "Tally".into(),
                        "Notes".into(),
                    ],
                    rows: track_rows,
                }),
                DocBlock::Table(TableBlock {
                    caption: Some("Team tracking form".into()),
                    headers: c.team_tracking_columns.clone(),
                    rows: (0..15)
                        .map(|_| vec![String::new(); c.team_tracking_columns.len()])
                        .collect(),
                }),
            ],
        };
        let path = sandbox.new_output_file("casper_clusters_and_tracking.xlsx")?;
        std::fs::write(&path, phn_docs::xlsx::to_xlsx(&xdoc)?)?;
        files.push(sandbox.display_path(&path));

        let p = |t: &str| DocBlock::Paragraph(vec![Inline::plain(t)]);
        let h = |t: &str| DocBlock::Heading {
            level: 2,
            text: t.to_string(),
        };
        let list = |items: Vec<String>, ordered: bool| DocBlock::List {
            ordered,
            items: items.into_iter().map(|i| vec![Inline::plain(i)]).collect(),
        };
        let ddoc = Document {
            title: format!("CASPER team packet — {frame_name}"),
            subtitle: Some("Based on the CDC CASPER Toolkit, 3rd edition".into()),
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks: vec![
                h("1. Consent script (read to every selected household)"),
                list(c.consent_template.clone(), false),
                p(&c.consent_template_note),
                h("2. Selecting seven households in your cluster"),
                list(c.design.stage2.clone(), true),
                h("3. Interview tips"),
                list(c.interview_tips.items.clone(), false),
                h("4. Field logistics and safety"),
                list(c.team.iter().map(|t| t.text.clone()).collect(), false),
                h("5. Supplies checklist"),
                list(
                    c.supplies.items.iter().map(|s| format!("☐ {s}")).collect(),
                    false,
                ),
                h("6. Confidential referral form"),
                p(
                    "Use only for urgent needs when calling 9-1-1 is not appropriate. Never attach it to a questionnaire or tracking form; call headquarters right away.",
                ),
                DocBlock::Table(TableBlock {
                    caption: Some("Confidential referral form".into()),
                    headers: vec!["Field".into(), "Entry".into()],
                    rows: c
                        .referral_fields
                        .iter()
                        .map(|f| vec![f.clone(), String::new()])
                        .collect(),
                }),
                h("7. Questionnaire guidance"),
                list(
                    c.questionnaire.iter().map(|q| q.text.clone()).collect(),
                    false,
                ),
            ],
        };
        let path = sandbox.new_output_file("casper_team_packet.docx")?;
        std::fs::write(&path, phn_docs::docx::to_docx(&ddoc)?)?;
        files.push(sandbox.display_path(&path));
    }

    // Summary.
    let mut s = format!(
        "**CASPER plan — {frame_name}**\n\nFrame: {} census blocks with {} {} (2020 Census). Design: {k} clusters × {per} interviews = {} interviews{}. Seed **{seed}** (use the same seed to reproduce this selection).\n\n",
        md::num(frame.len() as f64, 0),
        md::num(frame_households as f64, 0),
        if occupied {
            "occupied households"
        } else {
            "housing units"
        },
        k * per,
        if modified { " (modified CASPER)" } else { "" }
    );
    let rows: Vec<Vec<String>> = clusters
        .iter()
        .map(|cl| {
            vec![
                cl.cluster.to_string(),
                cl.block_geoid.clone(),
                cl.town.clone(),
                cl.households.to_string(),
                if cl.times_selected > 1 {
                    format!("{} (×{})", cl.interviews_to_complete, cl.times_selected)
                } else {
                    cl.interviews_to_complete.to_string()
                },
                cl.sampling_interval.to_string(),
            ]
        })
        .collect();
    s.push_str(&md::table(
        &[
            "Cluster",
            "Census block",
            "Town",
            "Households",
            "Interviews",
            "Every nth household",
        ],
        &rows,
    ));
    if repeated > 0 {
        s.push_str(&format!("\n{repeated} block(s) were selected more than once; teams complete 7 interviews for each time selected.\n"));
    }
    s.push_str(&format!("\n**Weights:** {}\n", c.analysis.weight_formula));
    if let Some(a) = &analysis {
        s.push_str(&format!(
            "\n**Analysis of `{}`:** {} completed interviews in {} clusters; completion rate {}%{}{}. {}\n\n",
            a.file,
            a.completed_interviews,
            a.clusters_with_interviews,
            a.completion_rate,
            a.contact_rate.map(|r| format!("; contact rate {r}%")).unwrap_or_default(),
            a.cooperation_rate.map(|r| format!("; cooperation rate {r}%")).unwrap_or_default(),
            if a.weighted { "Weighted estimates with 95% confidence intervals (Taylor-series, clusters as primary sampling units)." } else { "UNWEIGHTED percentages only (completion below 80%)." }
        ));
        for q in &a.questions {
            s.push_str(&format!(
                "_{}_ (n = {}, {} clusters)\n\n",
                q.question, q.respondents, q.clusters
            ));
            let rows: Vec<Vec<String>> = q
                .estimates
                .iter()
                .map(|e| {
                    let f = |x: Option<f64>| x.map(|v| v.to_string()).unwrap_or_else(|| "—".into());
                    vec![
                        e.category.clone(),
                        e.frequency.clone(),
                        e.estimated_households
                            .map(|v| md::num(v, 0))
                            .unwrap_or_else(|| "—".into()),
                        f(e.percent),
                        match (e.ci_lower, e.ci_upper) {
                            (Some(l), Some(u)) => format!("{l}–{u}"),
                            _ => "—".into(),
                        },
                        f(e.design_effect),
                    ]
                })
                .collect();
            s.push_str(&md::table(
                &[
                    "Answer",
                    "Frequency",
                    "Estimated households",
                    "% of households",
                    "95% CI",
                    "Design effect",
                ],
                &rows,
            ));
            s.push('\n');
        }
        s.push_str("Cells with fewer than 5 observations are not shown (CASPER Toolkit).\n");
    }
    if !files.is_empty() {
        s.push_str(&format!(
            "\nMaterials written: {} (cluster list, tracking forms, team tracking form; consent script, household selection steps, interview tips, supplies, referral form, questionnaire guidance). Make cluster maps from the block GEOIDs in TIGERweb or GIS.\n",
            files.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(" and ")
        ));
    }

    caveats.push(
        "Clusters are 2020 census blocks; housing built or lost since April 2020 (or destroyed in \
         the disaster) is not reflected, and weights cannot correct for it."
            .into(),
    );
    caveats.push(
        "Confidence intervals use Taylor-series linearization with clusters as primary sampling \
         units (with replacement, one stratum) and t with clusters − 1 degrees of freedom; Epi \
         Info's complex sample frequencies may differ slightly."
            .into(),
    );
    caveats.extend(content::review_caveats(&[content::CASPER]));
    let mut sources = vec![
        blocks.source.clone(),
        Source::computed(
            "CASPER weights and Taylor-series cluster variance",
            "CDC CASPER Toolkit 3rd ed., section 4.2; Wolter, Introduction to Variance Estimation",
        ),
    ];
    if let Some(l) = local_src {
        sources.push(l);
    }
    sources.extend(content::sources_for(
        c.sources.keys().map(String::as_str),
        &c.sources,
    ));
    let suppressed = analysis.as_ref().is_some_and(|a| {
        a.questions
            .iter()
            .flat_map(|q| q.estimates.iter())
            .any(|e| e.frequency.starts_with('<'))
    });

    Ok(Envelope::new(
        s,
        CasperResult {
            frame: frame_name,
            household_measure: if occupied { "occupied households" } else { "total housing units" }.into(),
            frame_blocks: frame.len(),
            frame_households,
            small_block_share: (small * 1000.0).round() / 1000.0,
            seed,
            design: format!("{k} clusters × {per} interviews"),
            modified,
            clusters,
            selection_method: "Probability proportional to size with replacement: random household numbers 1..N (SplitMix64, rejection sampling) mapped to blocks by cumulative household counts (CASPER Toolkit 2.4).".into(),
            stage2_steps: c.design.stage2.clone(),
            weight_formula: c.analysis.weight_formula.clone(),
            response_rates: c.analysis.response_rates.clone(),
            consent_elements: c.consent_elements.items.clone(),
            questionnaire_guidance: content::cite_all(&c.questionnaire, &c.sources),
            team_guidance: content::cite_all(&c.team, &c.sources),
            supplies: c.supplies.items.clone(),
            analysis,
            files: files.clone(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        suppression_applied: args.results_path.is_some(),
        suppression_threshold: args.results_path.as_ref().map(|_| 5),
        identifiers_detected: identifiers,
        files_written: files,
        notes: if suppressed {
            vec!["Answer counts under 5 are shown as <5 and their estimates withheld.".into()]
        } else {
            Vec::new()
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix64_reference_values() {
        // Reference outputs for seed 0 (Vigna's splitmix64.c).
        let mut r = SplitMix64::new(0);
        assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(r.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(r.next_u64(), 0x06C4_5D18_8009_454F);
    }

    #[test]
    fn pps_selection_follows_size() {
        let mut rng = SplitMix64::new(42);
        let picks = select_pps(&[90, 10], 10_000, &mut rng);
        let big = picks.iter().filter(|&&i| i == 0).count() as f64 / 10_000.0;
        assert!((big - 0.9).abs() < 0.02, "{big}");
        // Boundaries: household numbers 1-3 are block 0, 4 is block 1.
        let cum = [3u32, 1];
        let mut r = SplitMix64::new(7);
        for _ in 0..100 {
            let i = select_pps(&cum, 1, &mut r)[0];
            assert!(i < 2);
        }
    }

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn plans_reproducibly_and_analyzes_with_weights() {
        let (dir, c) = ctx();
        let args = CasperArgs {
            area: Some("Fall River".into()),
            seed: Some(2026),
            materials: Some(false),
            ..Default::default()
        };
        let a = plan_casper_assessment(&c, args.clone()).unwrap().data;
        let b = plan_casper_assessment(&c, args).unwrap().data;
        assert_eq!(a.clusters, b.clusters);
        assert_eq!(a.clusters.iter().map(|x| x.times_selected).sum::<u32>(), 30);
        assert!(
            a.clusters
                .iter()
                .all(|x| x.block_geoid.starts_with("25005"))
        );
        assert!(a.frame_households > 40_000);
        assert!(!a.modified);

        // Returned data: 30 clusters, 6 interviews each = 180 (85.7%).
        let mut csv = String::from("cluster,damage,generator\n");
        for cl in 1..=30 {
            for i in 0..6 {
                let damage = if i < cl % 4 { "Damaged" } else { "Not damaged" };
                let gen_ = if cl % 10 == 0 && i == 0 { "Yes" } else { "No" };
                csv.push_str(&format!("{cl},{damage},{gen_}\n"));
            }
        }
        std::fs::write(dir.path().join("data/casper.csv"), csv).unwrap();
        let env = plan_casper_assessment(
            &c,
            CasperArgs {
                area: Some("Fall River".into()),
                seed: Some(2026),
                materials: Some(true),
                results_path: Some("casper.csv".into()),
                questions: vec!["damage".into(), "generator".into()],
                frame_households: Some(43_214.0),
                tracking: Some(TrackingTotals {
                    attempted: Some(300),
                    contacted: Some(200),
                    completed: None,
                }),
                ..Default::default()
            },
        )
        .unwrap();
        let an = env.data.analysis.as_ref().unwrap();
        assert!(an.weighted);
        assert_eq!(an.completion_rate, 85.7);
        assert_eq!(an.contact_rate, Some(60.0));
        assert_eq!(an.cooperation_rate, Some(90.0));
        let dmg = &an.questions[0];
        let damaged = dmg
            .estimates
            .iter()
            .find(|e| e.category == "Damaged")
            .unwrap();
        // Equal interviews per cluster → equal weights → estimated households = 60 × 43214/(6×30).
        // 0-3 damaged per cluster (cluster number mod 4): 45 of 180.
        assert_eq!(damaged.frequency, "45");
        assert_eq!(
            damaged.estimated_households,
            Some((45.0f64 * 43_214.0 / 180.0).round())
        );
        assert_eq!(damaged.percent, Some(25.0));
        assert!(damaged.ci_lower.unwrap() < 25.0 && damaged.ci_upper.unwrap() > 25.0);
        assert!(damaged.design_effect.unwrap() > 1.0);
        let yes = an.questions[1]
            .estimates
            .iter()
            .find(|e| e.category == "Yes")
            .unwrap();
        assert_eq!(yes.frequency, "<5");
        assert!(yes.percent.is_none());
        assert_eq!(env.data.files.len(), 2);
        assert!(env.privacy.suppression_applied);
    }

    #[test]
    fn small_frames_and_bad_inputs() {
        let (_d, c) = ctx();
        // A frame with fewer blocks than clusters is refused.
        let e = plan_casper_assessment(
            &c,
            CasperArgs {
                area: Some("Gosnold".into()),
                exclude_blocks_under: Some(1_000_000),
                seed: Some(1),
                materials: Some(false),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("not be a CASPER"));
        assert!(
            plan_casper_assessment(
                &c,
                CasperArgs {
                    area: Some("Massachusetts".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            plan_casper_assessment(
                &c,
                CasperArgs {
                    area: Some("Salem".into()),
                    household_measure: Some("people".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
        let m = plan_casper_assessment(
            &c,
            CasperArgs {
                towns: vec!["Salem".into(), "Peabody".into()],
                clusters: Some(35),
                seed: Some(3),
                materials: Some(false),
                household_measure: Some("occupied".into()),
                ..Default::default()
            },
        )
        .unwrap()
        .data;
        assert!(m.modified);
        assert_eq!(m.clusters.iter().map(|x| x.times_selected).sum::<u32>(), 35);
    }
}
