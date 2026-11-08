//! CORE-02 `classify_intervention_wheel`: classifies public health nursing
//! activities by the 17 interventions, 5 wedges, and 3 levels of practice of
//! the Public Health Intervention Wheel (Minnesota Department of Health), for
//! reporting and workload analysis. Activity text is never returned.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::core::practice::{self, CitedNote, CitedText, Confidence, ContentSource};
use crate::md;

/// Content id.
pub const WHEEL_CONTENT: &str = "public-health-intervention-wheel";

#[derive(Debug, Clone, Deserialize)]
struct Wedge {
    id: String,
    label: String,
    interventions: Vec<String>,
    source: String,
    section: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct Level {
    id: String,
    label: String,
    definition: String,
    source: String,
    section: String,
    confidence: Confidence,
    cues: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Intervention {
    id: String,
    name: String,
    wedge: String,
    definition: String,
    source: String,
    section: String,
    confidence: Confidence,
    cues: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct WheelContent {
    meta: ContentMeta,
    sources: BTreeMap<String, ContentSource>,
    overview: CitedText,
    wedges: Vec<Wedge>,
    levels: Vec<Level>,
    interventions: Vec<Intervention>,
}

static CONTENT: LazyLock<WheelContent> =
    LazyLock::new(|| practice::load(WHEEL_CONTENT).expect("wheel content matches the Rust types"));

fn content() -> &'static WheelContent {
    &CONTENT
}

/// Arguments for `classify_intervention_wheel`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct WheelArgs {
    /// Short activity descriptions to classify ("flu clinic at the senior
    /// center", "drafted a tobacco regulation"). Leave out client names; results
    /// refer to activities by number only.
    pub activities: Vec<String>,
    /// An activity log (CSV/XLSX) in /data to classify; results per activity go
    /// to /data/output and only counts (and hours) are returned.
    pub path: Option<String>,
    /// Sheet name for an XLSX log.
    pub sheet: Option<String>,
    /// Column with the activity description (default: Activity, Description,
    /// Task, or Notes).
    pub activity_column: Option<String>,
    /// Column with hours spent (optional; summed by intervention).
    pub hours_column: Option<String>,
    /// Optional category column for counts by group (program, role, month).
    pub group_by: Option<String>,
}

/// An intervention in the taxonomy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InterventionInfo {
    pub id: String,
    pub name: String,
    pub wedge: String,
    pub definition: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// A level of practice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LevelInfo {
    pub id: String,
    pub label: String,
    pub definition: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// A wedge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WedgeInfo {
    pub id: String,
    pub label: String,
    pub interventions: Vec<String>,
    pub citation: String,
    pub confidence: Confidence,
}

/// Classification of one activity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ActivityClass {
    /// 1-based position in `activities`.
    pub activity: usize,
    /// Best intervention id, or "unclassified".
    pub intervention: String,
    pub intervention_name: String,
    pub wedge: String,
    /// individual_family, community, systems, or "unspecified".
    pub level: String,
    /// high, medium, or low (cue strength).
    pub match_strength: String,
    /// Other interventions the activity also suggests.
    pub also_consider: Vec<String>,
    /// Vocabulary cues that matched (this tool's terms, not the activity text).
    pub matched_terms: Vec<String>,
}

/// Counts for one intervention and level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WheelCount {
    pub group: String,
    pub intervention: String,
    pub level: String,
    /// Count of activities ("*" = suppressed).
    pub activities: String,
    /// Hours, when an hours column was given and the count is shown.
    pub hours: Option<f64>,
}

/// Batch summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WheelBatch {
    pub file: String,
    pub activities: usize,
    pub unclassified: usize,
    pub counts: Vec<WheelCount>,
    pub output_file: String,
    pub suppression_threshold: u32,
}

/// Result of `classify_intervention_wheel`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WheelResult {
    pub overview: CitedNote,
    pub classified: Vec<ActivityClass>,
    pub batch: Option<WheelBatch>,
    pub interventions: Vec<InterventionInfo>,
    pub wedges: Vec<WedgeInfo>,
    pub levels: Vec<LevelInfo>,
}

fn matches(s: &str, cue: &str) -> bool {
    let c = practice::padded_words(cue);
    let c = c.trim();
    !c.is_empty() && s.contains(&format!(" {c} "))
}

/// Classify one description.
fn classify(text: &str) -> (Option<&'static Intervention>, Vec<String>, String, String, Vec<String>) {
    let c = content();
    let s = practice::padded_words(text);
    let mut scored: Vec<(usize, &Intervention, Vec<String>)> = c
        .interventions
        .iter()
        .map(|i| {
            let hits: Vec<String> = i.cues.iter().filter(|q| matches(&s, q)).cloned().collect();
            (hits.len(), i, hits)
        })
        .filter(|(n, _, _)| *n > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    let level = {
        let has = |id: &str| {
            c.levels
                .iter()
                .find(|l| l.id == id)
                .is_some_and(|l| l.cues.iter().any(|q| matches(&s, q)))
        };
        if has("systems") {
            "systems"
        } else if has("community") {
            "community"
        } else if has("individual_family") {
            "individual_family"
        } else {
            "unspecified"
        }
    };
    match scored.first() {
        None => (None, Vec::new(), level.into(), "none".into(), Vec::new()),
        Some((n, best, hits)) => {
            let second = scored.get(1).map(|x| x.0).unwrap_or(0);
            let strength = if *n >= 2 && *n > second {
                "high"
            } else if *n > second {
                "medium"
            } else {
                "low"
            };
            let also: Vec<String> = scored
                .iter()
                .skip(1)
                .take(3)
                .map(|(_, i, _)| i.name.clone())
                .collect();
            (Some(*best), also, level.into(), strength.into(), hits.clone())
        }
    }
}

fn taxonomy() -> (Vec<InterventionInfo>, Vec<WedgeInfo>, Vec<LevelInfo>) {
    let c = content();
    (
        c.interventions
            .iter()
            .map(|i| InterventionInfo {
                id: i.id.clone(),
                name: i.name.clone(),
                wedge: i.wedge.clone(),
                definition: i.definition.clone(),
                citation: practice::cite(&c.sources, &i.source, &i.section),
                confidence: i.confidence,
            })
            .collect(),
        c.wedges
            .iter()
            .map(|w| WedgeInfo {
                id: w.id.clone(),
                label: w.label.clone(),
                interventions: w
                    .interventions
                    .iter()
                    .filter_map(|id| c.interventions.iter().find(|i| &i.id == id))
                    .map(|i| i.name.clone())
                    .collect(),
                citation: practice::cite(&c.sources, &w.source, &w.section),
                confidence: w.confidence,
            })
            .collect(),
        c.levels
            .iter()
            .map(|l| LevelInfo {
                id: l.id.clone(),
                label: l.label.clone(),
                definition: l.definition.clone(),
                citation: practice::cite(&c.sources, &l.source, &l.section),
                confidence: l.confidence,
            })
            .collect(),
    )
}

fn level_label(id: &str) -> String {
    content()
        .levels
        .iter()
        .find(|l| l.id == id)
        .map(|l| l.label.clone())
        .unwrap_or_else(|| "unspecified".into())
}

fn caveats(c: &WheelContent) -> Vec<String> {
    let mut v: Vec<String> = practice::review_caveat(&c.meta).into_iter().collect();
    v.push(
        "MDH's Wheel page could not be retrieved; four intervention definitions and the level \
         definitions are low confidence (see each item's confidence). Confirm against Public \
         Health Interventions: Applications for Public Health Nursing Practice (MDH, 2nd ed., \
         2019)."
            .into(),
    );
    v.push(
        "Classifications come from keyword cues; one activity often uses several interventions \
         (see also_consider). Confirm the intervention and the level of practice before \
         reporting."
            .into(),
    );
    v
}

/// Classify activities by the Public Health Intervention Wheel.
pub fn classify_intervention_wheel(
    ctx: &Context,
    args: WheelArgs,
) -> Result<Envelope<WheelResult>> {
    let c = content();
    let (interventions, wedges, levels) = taxonomy();
    let overview = practice::note(&c.overview, &c.sources);
    let mut sources = practice::sources_for(
        &c.sources,
        ["mdh-wheel", "keller-2004", "schaffer-2022", "leclair-2024", "kovacevic-2024"],
    );
    if let Some(path) = args.path.as_deref().filter(|p| !p.trim().is_empty()) {
        return batch(ctx, &args, path, overview, interventions, wedges, levels);
    }
    if args.activities.len() > 200 {
        return Err(PhnError::InvalidInput(
            "give at most 200 activities (use path for a longer log)".into(),
        ));
    }
    let mut classified = Vec::new();
    for (i, a) in args.activities.iter().enumerate() {
        let (best, also, level, strength, hits) = classify(a);
        classified.push(ActivityClass {
            activity: i + 1,
            intervention: best.map(|b| b.id.clone()).unwrap_or_else(|| "unclassified".into()),
            intervention_name: best.map(|b| b.name.clone()).unwrap_or_else(|| "Unclassified".into()),
            wedge: best.map(|b| b.wedge.clone()).unwrap_or_default(),
            level,
            match_strength: strength,
            also_consider: also,
            matched_terms: hits,
        });
    }
    let mut s = String::from("**Public Health Intervention Wheel**\n\n");
    if classified.is_empty() {
        s.push_str(&format!("{}\n\n", c.overview.text));
        for w in &wedges {
            s.push_str(&format!("- **{}:** {}\n", w.label, w.interventions.join(", ")));
        }
        s.push_str("\n**Levels of practice**\n\n");
        for l in &levels {
            s.push_str(&format!("- **{}:** {}\n", l.label, l.definition));
        }
    } else {
        let rows: Vec<Vec<String>> = classified
            .iter()
            .map(|a| {
                vec![
                    a.activity.to_string(),
                    a.intervention_name.clone(),
                    a.wedge.clone(),
                    level_label(&a.level),
                    a.match_strength.clone(),
                    a.also_consider.join(", "),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["#", "Intervention", "Wedge", "Level", "Match", "Also consider"],
            &rows,
        ));
        let mut tally: BTreeMap<String, usize> = BTreeMap::new();
        for a in &classified {
            *tally.entry(a.intervention_name.clone()).or_default() += 1;
        }
        s.push_str(&format!(
            "\nBy intervention: {}.\n",
            tally
                .iter()
                .map(|(k, v)| format!("{k} {v}"))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    sources.truncate(5);
    Ok(Envelope::new(
        s,
        WheelResult {
            overview,
            classified,
            batch: None,
            interventions,
            wedges,
            levels,
        },
    )
    .sources(sources)
    .caveats(caveats(c))
    .privacy(Privacy {
        notes: vec![
            "Activity descriptions are not repeated; results refer to activities by number."
                .into(),
        ],
        ..Privacy::none()
    }))
}

fn batch(
    ctx: &Context,
    args: &WheelArgs,
    path: &str,
    overview: CitedNote,
    interventions: Vec<InterventionInfo>,
    wedges: Vec<WedgeInfo>,
    levels: Vec<LevelInfo>,
) -> Result<Envelope<WheelResult>> {
    let c = content();
    let lt = crate::localdata::load(ctx, path, args.sheet.as_deref())?;
    let t = &lt.table;
    // The description column is read locally only (it may hold free text), so
    // it is located directly rather than through the identifier guard.
    let act_c = match args.activity_column.as_deref() {
        Some(name) => t.column_index(name).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "'{}' has no column named '{name}'",
                lt.label
            ))
        })?,
        None => practice::find_col(
            t,
            &["activity", "activities", "description", "task", "notes", "activity_description"],
        )
        .ok_or_else(|| {
            PhnError::InvalidInput(
                "name the column that describes each activity with activity_column (default: \
                 Activity, Description, Task, or Notes)"
                    .into(),
            )
        })?,
    };
    let hours_c = match args.hours_column.as_deref() {
        Some(h) => Some(lt.column(h)?),
        None => practice::find_col(t, &["hours", "time_hours", "hrs"]),
    };
    let groups = match args.group_by.as_deref() {
        Some(g) => {
            let gc = lt.column(g)?;
            Some(lt.categorical(gc)?)
        }
        None => None,
    };
    let (hours, _) = match hours_c {
        Some(h) => {
            let (v, share) = lt.numeric(h);
            (Some(v), share)
        }
        None => (None, 0.0),
    };
    let mut out_rows = Vec::new();
    // (group, intervention name, level) -> (count, hours)
    let mut agg: BTreeMap<(String, String, String), (u64, f64)> = BTreeMap::new();
    let mut n = 0;
    let mut unclassified = 0;
    for (ri, row) in t.rows.iter().enumerate() {
        let text = row.get(act_c).map(String::as_str).unwrap_or("").trim();
        if text.is_empty() {
            continue;
        }
        n += 1;
        let (best, also, level, strength, _) = classify(text);
        if best.is_none() {
            unclassified += 1;
        }
        let name = best.map(|b| b.name.clone()).unwrap_or_else(|| "Unclassified".into());
        let h = hours.as_ref().and_then(|v| v[ri]).unwrap_or(0.0);
        let g = groups
            .as_ref()
            .and_then(|g| g[ri].clone())
            .unwrap_or_else(|| "(blank)".into());
        for key in [g.clone(), "All".into()] {
            if key != "All" && groups.is_none() {
                continue;
            }
            let e = agg.entry((key, name.clone(), level.clone())).or_default();
            e.0 += 1;
            e.1 += h;
        }
        let mut line = row.clone();
        line.extend([
            name,
            best.map(|b| b.wedge.clone()).unwrap_or_default(),
            level_label(&level),
            strength,
            also.join("; "),
        ]);
        out_rows.push(line);
    }
    let mut headers = t.headers.clone();
    headers.extend(
        ["Intervention", "Wedge", "Level of practice", "Match", "Also consider"].map(String::from),
    );
    let h: Vec<&str> = headers.iter().map(String::as_str).collect();
    let doc = practice::document(
        ctx,
        "Activities classified by the Public Health Intervention Wheel",
        vec![
            practice::para(
                "Classified from keyword cues; confirm each intervention and level of practice.",
            ),
            practice::table("Activities", &h, out_rows),
        ],
    );
    let written = practice::write_document(ctx, &doc, "intervention_wheel_classified", &["xlsx"])?;
    let out = written.first().cloned().unwrap_or_default();

    let threshold = ctx.suppression_threshold();
    let mut counts = Vec::new();
    let mut any = false;
    let mut group_names: Vec<String> = agg.keys().map(|k| k.0.clone()).collect();
    group_names.sort();
    group_names.dedup();
    group_names.retain(|g| g != "All");
    group_names.insert(0, "All".into());
    for g in &group_names {
        let cells: Vec<(&(String, String, String), &(u64, f64))> =
            agg.iter().filter(|(k, _)| &k.0 == g).collect();
        let raw: Vec<(String, u64)> = cells
            .iter()
            .map(|(k, v)| (format!("{}|{}", k.1, k.2), v.0))
            .collect();
        let (shown, a) = practice::suppressed_counts(&raw, threshold);
        any |= a;
        for ((k, v), (_, val)) in cells.iter().zip(shown) {
            counts.push(WheelCount {
                group: g.clone(),
                intervention: k.1.clone(),
                level: k.2.clone(),
                hours: (hours.is_some() && val != "*").then_some((v.1 * 10.0).round() / 10.0),
                activities: val,
            });
        }
    }
    let mut s = format!(
        "**Public Health Intervention Wheel — {n} activities in `{}`**\n\nPer-activity results \
         were written to `{out}` (descriptions not shown here).\n\n",
        lt.label
    );
    let rows: Vec<Vec<String>> = counts
        .iter()
        .filter(|r| r.group == "All")
        .map(|r| {
            vec![
                r.intervention.clone(),
                level_label(&r.level),
                r.activities.clone(),
                r.hours.map(|h| md::num(h, 1)).unwrap_or_default(),
            ]
        })
        .collect();
    s.push_str(&md::table(&["Intervention", "Level", "Activities", "Hours"], &rows));
    if any {
        s.push_str(&format!(
            "\n`*` = suppressed (counts from 1 to {} and cells that would reveal them).\n",
            threshold - 1
        ));
    }
    let mut cav = caveats(c);
    if unclassified > 0 {
        cav.push(format!(
            "{unclassified} activit(ies) matched no intervention cue (listed as Unclassified in \
             the output file)."
        ));
    }
    let sources: Vec<Source> = std::iter::once(lt.source())
        .chain(practice::sources_for(
            &c.sources,
            ["mdh-wheel", "keller-2004", "leclair-2024", "kovacevic-2024"],
        ))
        .collect();
    Ok(Envelope::new(
        s,
        WheelResult {
            overview,
            classified: Vec::new(),
            batch: Some(WheelBatch {
                file: lt.label.clone(),
                activities: n,
                unclassified,
                counts,
                output_file: out,
                suppression_threshold: threshold,
            }),
            interventions,
            wedges,
            levels,
        },
    )
    .sources(sources)
    .caveats(cav)
    .privacy(Privacy {
        row_level_data: false,
        suppression_applied: true,
        suppression_threshold: Some(threshold),
        identifiers_detected: lt.identifier_categories(),
        files_written: written,
        notes: vec![
            "Activity descriptions stay in the output file; only counts and hours are returned."
                .into(),
        ],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(with_data: bool) -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        if with_data {
            std::fs::create_dir_all(dir.path().join("data")).unwrap();
        }
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn taxonomy_has_17_interventions_in_5_wedges() {
        let (i, w, l) = taxonomy();
        assert_eq!(i.len(), 17);
        assert_eq!(w.len(), 5);
        assert_eq!(l.len(), 3);
        let n: usize = w.iter().map(|w| w.interventions.len()).sum();
        assert_eq!(n, 17);
    }

    #[test]
    fn classifies_typical_activities() {
        let (_d, c) = ctx(false);
        let env = classify_intervention_wheel(
            &c,
            WheelArgs {
                activities: vec![
                    "Drafted a tobacco regulation for the board of health".into(),
                    "Measles outbreak contact tracing and case interview".into(),
                    "Taught a car seat safety class for new parents".into(),
                    "Referred the family to WIC and followed up by phone".into(),
                    "Convened the opioid task force".into(),
                ],
                ..WheelArgs::default()
            },
        )
        .unwrap();
        let d = &env.data.classified;
        assert_eq!(d[0].intervention, "policy_development_enforcement");
        assert_eq!(d[0].level, "systems");
        assert_eq!(d[1].intervention, "disease_investigation");
        assert_eq!(d[2].intervention, "health_teaching");
        assert_eq!(d[3].intervention, "referral_follow_up");
        assert_eq!(d[3].level, "individual_family");
        assert_eq!(d[4].intervention, "coalition_building");
        let json = serde_json::to_string(&env).unwrap();
        assert!(!json.contains("car seat safety class"));
    }

    #[test]
    fn batch_counts_with_hours() {
        let (dir, c) = ctx(true);
        let mut csv = String::from("Date,Activity,Hours,Program\n");
        for i in 0..6 {
            csv.push_str(&format!(
                "2026-10-0{},Home visit with Gwendolyn Quarrington for case management,1.5,MCH\n",
                i + 1
            ));
        }
        for i in 0..6 {
            csv.push_str(&format!("2026-10-1{i},Flu shots at the senior center clinic,3,Immunization\n"));
        }
        std::fs::write(dir.path().join("data/log.csv"), csv).unwrap();
        let env = classify_intervention_wheel(
            &c,
            WheelArgs {
                path: Some("log.csv".into()),
                group_by: Some("Program".into()),
                ..WheelArgs::default()
            },
        )
        .unwrap();
        let b = env.data.batch.as_ref().unwrap();
        assert_eq!(b.activities, 12);
        let cm = b
            .counts
            .iter()
            .find(|r| r.group == "All" && r.intervention == "Case management")
            .unwrap();
        assert_eq!(cm.activities, "6");
        assert_eq!(cm.hours, Some(9.0));
        let json = serde_json::to_string(&env).unwrap();
        assert!(!json.contains("Quarrington"));
    }
}
