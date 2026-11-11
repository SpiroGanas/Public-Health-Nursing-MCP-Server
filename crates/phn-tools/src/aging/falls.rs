//! AGE-01 `steadi_fall_risk_assessment`: CDC STEADI screening (Stay
//! Independent or the three key questions), the Timed Up and Go, 30-Second
//! Chair Stand (CDC below-average norms by age and sex), 4-Stage Balance
//! Test, and orthostatic blood pressure, turned into an intervention and
//! home-safety plan. Content: `content/aging/steadi_falls.yaml` (pending
//! review). Optionally writes the plan as DOCX or PDF to /data/output (no
//! identifiers are accepted).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::aging::content::{self as c10, FALLS, SourceRef};
use crate::context::Context;
use crate::md;

// ------------------------------------------------------------------ content

#[derive(Debug, Clone, Deserialize)]
struct SiItem {
    n: u32,
    points: u32,
    wording: String,
    why: String,
    factor: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StayIndependent {
    at_risk_score: u32,
    rule: String,
    attribution: String,
    items: Vec<SiItem>,
}

#[derive(Debug, Clone, Deserialize)]
struct KeyQuestion {
    #[allow(dead_code)]
    id: String,
    wording: String,
}

#[derive(Debug, Clone, Deserialize)]
struct KeyQuestions {
    rule: String,
    questions: Vec<KeyQuestion>,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Tug {
    at_risk_seconds: f64,
    rule: String,
    procedure: String,
    observations: Vec<String>,
    observations_note: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct NormRow {
    age_min: u32,
    age_max: u32,
    men: u32,
    women: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct ChairStand {
    procedure: String,
    rule: String,
    below_average: Vec<NormRow>,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Stage {
    n: u32,
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FourStage {
    hold_seconds: f64,
    stages: Vec<Stage>,
    rule: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Orthostatic {
    procedure: String,
    systolic_drop: f64,
    diastolic_drop: f64,
    rule: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Assess {
    factor: String,
    assess: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Intervention {
    factor: String,
    finding: String,
    actions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Check {
    hazard: String,
    fix: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Area {
    area: String,
    checks: Vec<Check>,
}

#[derive(Debug, Clone, Deserialize)]
struct HomeChecklist {
    areas: Vec<Area>,
    source: String,
    #[allow(dead_code)]
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CitedLine {
    text: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FallsContent {
    #[allow(dead_code)]
    meta: ContentMeta,
    sources: BTreeMap<String, SourceRef>,
    stay_independent: StayIndependent,
    key_questions: KeyQuestions,
    screen_frequency: CitedLine,
    tug: Tug,
    chair_stand: ChairStand,
    four_stage: FourStage,
    orthostatic: Orthostatic,
    assessments: Vec<Assess>,
    interventions: Vec<Intervention>,
    not_at_risk: Vec<String>,
    at_risk_follow_up: String,
    home_checklist: HomeChecklist,
    patient_actions: Vec<String>,
    patient_actions_source: String,
}

static CONTENT: LazyLock<FallsContent> =
    LazyLock::new(|| c10::load(FALLS).expect("STEADI content parses"));

// ------------------------------------------------------------------ arguments

/// Orthostatic blood pressure readings (mm Hg).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct OrthostaticInput {
    /// Systolic and diastolic after lying down 5 minutes.
    pub lying_systolic: Option<f64>,
    pub lying_diastolic: Option<f64>,
    /// After standing 1 minute.
    pub standing_1min_systolic: Option<f64>,
    pub standing_1min_diastolic: Option<f64>,
    /// After standing 3 minutes.
    pub standing_3min_systolic: Option<f64>,
    pub standing_3min_diastolic: Option<f64>,
    /// Lightheadedness or dizziness on standing.
    pub symptoms: bool,
}

/// Arguments for `steadi_fall_risk_assessment`. No names or identifiers.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct FallRiskArgs {
    /// Stay Independent brochure answers in order (12 items; true = yes).
    /// Use null for an unanswered item.
    pub stay_independent: Vec<Option<bool>>,
    /// Three key questions (alternative screen): feels unsteady when
    /// standing or walking?
    pub unsteady: Option<bool>,
    /// Worries about falling?
    pub worries_about_falling: Option<bool>,
    /// Fell in the past year?
    pub fell_past_year: Option<bool>,
    /// Number of falls in the past year, if known.
    pub falls_count: Option<u32>,
    /// Injured in a fall in the past year.
    pub fall_injury: Option<bool>,
    /// Age in years (needed for the chair-stand norms).
    pub age_years: Option<u32>,
    /// "female" or "male" (the CDC chair-stand norms are by sex).
    pub sex: Option<String>,
    /// Timed Up and Go time in seconds.
    pub tug_seconds: Option<f64>,
    /// TUG gait observations (e.g. "shuffling", "loss of balance").
    pub tug_observations: Vec<String>,
    /// 30-Second Chair Stand: number of full stands.
    pub chair_stands: Option<u32>,
    /// The person had to use their arms to stand (score 0).
    pub chair_used_arms: bool,
    /// 4-Stage Balance Test: seconds held at each position in order (side by
    /// side, semi-tandem, tandem, one foot); stop at the first under 10.
    pub balance_seconds: Vec<f64>,
    /// Orthostatic blood pressure readings.
    pub orthostatic: Option<OrthostaticInput>,
    /// Other risk factors found: medications, vision, feet, vitamin_d,
    /// comorbidities, home_hazards, urgency (free text is matched).
    pub other_risk_factors: Vec<String>,
    /// Home hazards found (free text such as "throw rugs", "no grab bars"),
    /// matched to CDC's Check for Safety fixes. Use "all" for the whole checklist.
    pub home_hazards: Vec<String>,
    /// Write the plan to /data/output: "docx" or "pdf".
    pub plan_format: Option<String>,
}

// ------------------------------------------------------------------ results

/// The screening step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScreenOut {
    /// stay_independent, key_questions, or none.
    pub method: String,
    pub score: Option<u32>,
    pub max_score: Option<u32>,
    /// Null when the screen is incomplete or not given.
    pub at_risk: Option<bool>,
    pub reason: String,
    /// Items answered yes, with why they matter.
    pub positive_items: Vec<String>,
    pub citation: String,
}

/// One gait, strength, balance, or blood pressure test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TestOut {
    pub test: String,
    pub value: String,
    pub threshold: String,
    /// at_risk, not_at_risk, or no_norm.
    pub result: String,
    pub interpretation: String,
    pub citation: String,
}

/// A risk factor and the evidence for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FactorOut {
    pub factor: String,
    pub evidence: Vec<String>,
}

/// Recommended interventions for a factor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InterventionOut {
    pub factor: String,
    pub finding: String,
    pub actions: Vec<String>,
}

/// A home hazard and its fix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HazardOut {
    pub area: String,
    pub hazard: String,
    pub fix: String,
    /// What the nurse entered that matched (empty for the full checklist).
    pub matched_from: String,
}

/// Result of `steadi_fall_risk_assessment`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FallRiskResult {
    pub screen: ScreenOut,
    pub tests: Vec<TestOut>,
    /// at_risk, not_at_risk, or incomplete.
    pub overall: String,
    pub risk_factors: Vec<FactorOut>,
    pub intervention_plan: Vec<InterventionOut>,
    /// STEADI assessments still to do for a person screened at risk.
    pub assessments_to_do: Vec<String>,
    /// Prevention for people screened not at risk.
    pub prevention: Vec<String>,
    pub follow_up: String,
    pub home_safety: Vec<HazardOut>,
    /// Home hazards entered that did not match the checklist.
    pub unmatched_hazards: Vec<String>,
    pub patient_actions: Vec<String>,
    /// How to do each STEADI test (CDC instructions).
    pub test_procedures: Vec<ProcedureOut>,
    pub plan_file: Option<String>,
}

/// How to do a test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProcedureOut {
    pub test: String,
    pub procedure: String,
    pub citation: String,
}

// ------------------------------------------------------------------ helpers

/// CDC's below-average cutoff for a chair-stand score (score < cutoff).
pub fn chair_stand_cutoff(age: u32, female: bool) -> Option<u32> {
    CONTENT
        .chair_stand
        .below_average
        .iter()
        .find(|r| age >= r.age_min && age <= r.age_max)
        .map(|r| if female { r.women } else { r.men })
}

fn parse_sex(s: Option<&str>) -> Result<Option<bool>> {
    match s.map(phn_core::text::normalize) {
        None => Ok(None),
        Some(v) if v.is_empty() => Ok(None),
        Some(v) if matches!(v.as_str(), "f" | "female" | "woman" | "women") => Ok(Some(true)),
        Some(v) if matches!(v.as_str(), "m" | "male" | "man" | "men") => Ok(Some(false)),
        Some(v) => Err(PhnError::InvalidInput(format!(
            "sex '{v}' is not recognized; the CDC chair-stand norms are published for men and women (use \"female\" or \"male\", or leave it out)"
        ))),
    }
}

fn factor_label(f: &str) -> &'static str {
    match f {
        "fall_history" => "fall in the past year",
        "gait_balance" => "gait and balance",
        "fear_of_falling" => "fear of falling",
        "strength" => "leg strength",
        "urgency" => "urinary urgency",
        "feet" => "feet and footwear",
        "medications" => "medications",
        "comorbidities" => "comorbidities (e.g., depression)",
        "home_hazards" => "home hazards",
        "orthostatic" => "orthostatic hypotension",
        "vision" => "vision",
        "vitamin_d" => "vitamin D",
        _ => "other",
    }
}

fn normalize_factor(s: &str) -> Option<&'static str> {
    let n = phn_core::text::normalize(s);
    let has = |k: &[&str]| k.iter().any(|w| n.contains(w));
    Some(if has(&["medic", "drug", "sedat", "benzo", "polypharm"]) {
        "medications"
    } else if has(&["vision", "eye", "sight", "cataract"]) {
        "vision"
    } else if has(&["feet", "foot", "shoe", "neuropath", "numb"]) {
        "feet"
    } else if has(&["vitamin"]) {
        "vitamin_d"
    } else if has(&[
        "depress",
        "osteopor",
        "comorbid",
        "dementia",
        "parkinson",
        "stroke",
    ]) {
        "comorbidities"
    } else if has(&["home", "hazard", "rug", "stairs", "clutter"]) {
        "home_hazards"
    } else if has(&["urgen", "incontin", "toilet", "bathroom"]) {
        "urgency"
    } else if has(&[
        "orthostat",
        "postural",
        "dizz",
        "lighthead",
        "blood pressure",
    ]) {
        "orthostatic"
    } else if has(&["fear", "worr"]) {
        "fear_of_falling"
    } else if has(&["gait", "balance", "unsteady", "walker", "cane"]) {
        "gait_balance"
    } else if has(&["strength", "weak", "muscle"]) {
        "strength"
    } else {
        return None;
    })
}

fn cite(key: &str, section: &str) -> String {
    c10::citation(&CONTENT.sources, key, section)
}

// ------------------------------------------------------------------ tool

/// Run a STEADI fall risk assessment and build an intervention plan.
pub fn steadi_fall_risk_assessment(
    ctx: &Context,
    args: FallRiskArgs,
) -> Result<Envelope<FallRiskResult>> {
    let c = &*CONTENT;
    let female = parse_sex(args.sex.as_deref())?;
    let mut factors: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut add = |f: &str, why: String| factors.entry(f.to_string()).or_default().push(why);

    // A count of falls answers the "fell in the past year" question.
    let fell_past_year = match (args.fell_past_year, args.falls_count) {
        (Some(false), Some(n)) if n > 0 => {
            return Err(PhnError::InvalidInput(format!(
                "fell_past_year is false but falls_count is {n}; correct one of them"
            )));
        }
        (None, Some(n)) => Some(n > 0),
        (f, _) => f,
    };

    // ---- 1. Screen
    let si = &c.stay_independent;
    let screen = if !args.stay_independent.is_empty() {
        if args.stay_independent.len() > si.items.len() {
            return Err(PhnError::InvalidInput(format!(
                "the Stay Independent checklist has {} items, but {} answers were given",
                si.items.len(),
                args.stay_independent.len()
            )));
        }
        let mut score = 0;
        let mut positive = Vec::new();
        let mut missing = 0;
        for item in &si.items {
            match args
                .stay_independent
                .get(item.n as usize - 1)
                .copied()
                .flatten()
            {
                Some(true) => {
                    score += item.points;
                    positive.push(format!(
                        "{} ({} pt) — {}",
                        item.wording, item.points, item.why
                    ));
                    add(
                        &item.factor,
                        format!("Stay Independent: \"{}\"", item.wording),
                    );
                }
                Some(false) => {}
                None => missing += 1,
            }
        }
        let fell = args.stay_independent.first().copied().flatten() == Some(true)
            || fell_past_year == Some(true);
        let (at_risk, reason) = if score >= si.at_risk_score {
            (
                Some(true),
                format!("Score {score} is {} or more.", si.at_risk_score),
            )
        } else if fell {
            (
                Some(true),
                format!(
                    "Score {score} is under {}, but the person fell in the past year, so they are at risk.",
                    si.at_risk_score
                ),
            )
        } else if missing > 0 {
            (
                None,
                format!("Score {score} with {missing} item(s) unanswered; complete the checklist."),
            )
        } else {
            (
                Some(false),
                format!(
                    "Score {score} is under {} and no fall in the past year.",
                    si.at_risk_score
                ),
            )
        };
        ScreenOut {
            method: "stay_independent".into(),
            score: Some(score),
            max_score: Some(si.items.iter().map(|i| i.points).sum()),
            at_risk,
            reason,
            positive_items: positive,
            citation: format!(
                "{} {} ({})",
                si.rule.trim(),
                cite("stay-independent", "Check Your Risk for Falling"),
                si.attribution.trim()
            ),
        }
    } else if args.unsteady.is_some()
        || args.worries_about_falling.is_some()
        || fell_past_year.is_some()
    {
        let answers = [args.unsteady, args.worries_about_falling, fell_past_year];
        let mut positive = Vec::new();
        for (q, a) in c.key_questions.questions.iter().zip(answers) {
            if a == Some(true) {
                positive.push(q.wording.clone());
            }
        }
        if args.unsteady == Some(true) {
            add(
                "gait_balance",
                "Feels unsteady when standing or walking".into(),
            );
        }
        if args.worries_about_falling == Some(true) {
            add("fear_of_falling", "Worries about falling".into());
        }
        if fell_past_year == Some(true) {
            add("fall_history", "Fell in the past year".into());
        }
        let at_risk = if !positive.is_empty() {
            Some(true)
        } else if answers.iter().all(Option::is_some) {
            Some(false)
        } else {
            None
        };
        ScreenOut {
            method: "key_questions".into(),
            score: None,
            max_score: None,
            at_risk,
            reason: match at_risk {
                Some(true) => "Yes to at least one key question.".into(),
                Some(false) => "No to all three key questions.".into(),
                None => "Not all three key questions were answered.".into(),
            },
            positive_items: positive,
            citation: format!(
                "{} ({})",
                cite(&c.key_questions.source, &c.key_questions.section),
                c.key_questions.rule
            ),
        }
    } else {
        ScreenOut {
            method: "none".into(),
            score: None,
            max_score: None,
            at_risk: None,
            reason: "No screening answers were given.".into(),
            positive_items: Vec::new(),
            citation: cite("steadi-algorithm", "1 Screen"),
        }
    };
    if fell_past_year == Some(true) && screen.method == "stay_independent" {
        add("fall_history", "Fell in the past year".into());
    }
    if let Some(n) = args.falls_count.filter(|n| *n > 0) {
        add(
            "fall_history",
            format!(
                "{n} fall(s) in the past year{}",
                if args.fall_injury == Some(true) {
                    ", with injury"
                } else {
                    ""
                }
            ),
        );
    }

    // ---- 2. Tests
    let mut tests = Vec::new();
    let mut any_test_risk = false;
    if let Some(t) = args.tug_seconds {
        if !(1.0..=600.0).contains(&t) {
            return Err(PhnError::InvalidInput(format!(
                "a TUG time of {t} seconds is not plausible; give the time in seconds"
            )));
        }
        let risk = t >= c.tug.at_risk_seconds;
        any_test_risk |= risk;
        if risk {
            add(
                "gait_balance",
                format!("Timed Up and Go {t:.1} s (12 s or more)"),
            );
        }
        let mut interp = c.tug.rule.clone();
        if !args.tug_observations.is_empty() {
            interp.push_str(&format!(
                " Observed: {}. {}",
                args.tug_observations.join(", "),
                c.tug.observations_note
            ));
            add(
                "gait_balance",
                format!("Gait observations: {}", args.tug_observations.join(", ")),
            );
        }
        tests.push(TestOut {
            test: "Timed Up and Go".into(),
            value: format!("{t:.1} seconds"),
            threshold: format!("{} seconds or more = at risk", c.tug.at_risk_seconds),
            result: if risk { "at_risk" } else { "not_at_risk" }.into(),
            interpretation: interp,
            citation: cite(&c.tug.source, &c.tug.section),
        });
    }
    if args.chair_stands.is_some() || args.chair_used_arms {
        let stands = if args.chair_used_arms {
            0
        } else {
            args.chair_stands.unwrap_or(0)
        };
        if stands > 60 {
            return Err(PhnError::InvalidInput(format!(
                "{stands} chair stands in 30 seconds is not plausible"
            )));
        }
        let cutoff = match (args.age_years, female) {
            (Some(a), Some(f)) => chair_stand_cutoff(a, f),
            _ => None,
        };
        let (result, threshold, interp) = match cutoff {
            Some(cut) => {
                let risk = stands < cut;
                any_test_risk |= risk;
                if risk {
                    add(
                        "strength",
                        format!("30-Second Chair Stand {stands} (below average: under {cut})"),
                    );
                }
                (
                    if risk { "at_risk" } else { "not_at_risk" },
                    format!(
                        "below average if under {cut} for {} aged {}",
                        if female == Some(true) { "women" } else { "men" },
                        args.age_years.unwrap_or(0)
                    ),
                    c.chair_stand.rule.clone(),
                )
            }
            None => (
                "no_norm",
                "CDC norms cover ages 60–94 by sex".to_string(),
                if args.age_years.is_none() || female.is_none() {
                    "Give age and sex to compare with CDC's below-average scores.".to_string()
                } else {
                    "CDC publishes no chair-stand norm for this age; interpret clinically."
                        .to_string()
                },
            ),
        };
        if args.chair_used_arms {
            add(
                "strength",
                "Needed arms to stand in the chair-stand test (score 0)".into(),
            );
        }
        tests.push(TestOut {
            test: "30-Second Chair Stand".into(),
            value: format!(
                "{stands} stand(s){}",
                if args.chair_used_arms {
                    " (used arms: recorded as 0)"
                } else {
                    ""
                }
            ),
            threshold,
            result: result.into(),
            interpretation: interp,
            citation: cite(&c.chair_stand.source, &c.chair_stand.section),
        });
    }
    if !args.balance_seconds.is_empty() {
        if args.balance_seconds.len() > 4
            || args
                .balance_seconds
                .iter()
                .any(|s| !(0.0..=600.0).contains(s))
        {
            return Err(PhnError::InvalidInput("balance_seconds takes up to 4 times in seconds (side by side, semi-tandem, tandem, one foot)".into()));
        }
        let hold = c.four_stage.hold_seconds;
        let failed_at = args.balance_seconds.iter().position(|s| *s < hold);
        let tandem_ok = args.balance_seconds.get(2).is_some_and(|s| *s >= hold);
        let (result, interp) = match failed_at {
            Some(i) if i <= 2 => {
                any_test_risk = true;
                add(
                    "gait_balance",
                    format!(
                        "4-Stage Balance: could not hold stage {} ({}) for 10 seconds",
                        i + 1,
                        c.four_stage.stages[i].name
                    ),
                );
                (
                    "at_risk",
                    format!(
                        "Could not hold the {} position for {hold} seconds, so cannot hold the tandem stand: increased risk of falling.",
                        c.four_stage.stages[i].name
                    ),
                )
            }
            _ if tandem_ok => (
                "not_at_risk",
                "Held the tandem stand for at least 10 seconds.".to_string(),
            ),
            _ => (
                "no_norm",
                "The tandem stand time was not given; the result depends on stage 3.".to_string(),
            ),
        };
        tests.push(TestOut {
            test: "4-Stage Balance Test".into(),
            value: args
                .balance_seconds
                .iter()
                .enumerate()
                .map(|(i, s)| format!("stage {}: {s:.0} s", i + 1))
                .collect::<Vec<_>>()
                .join("; "),
            threshold: "cannot hold the tandem stand (stage 3) for 10 seconds = increased risk"
                .into(),
            result: result.into(),
            interpretation: format!(
                "{interp} {}",
                c.four_stage.rule.split(". ").last().unwrap_or_default()
            ),
            citation: cite(&c.four_stage.source, &c.four_stage.section),
        });
    }
    if let Some(o) = &args.orthostatic {
        let mut drops = Vec::new();
        let mut abnormal = o.symptoms;
        for (label, s, d) in [
            (
                "1 minute",
                o.standing_1min_systolic,
                o.standing_1min_diastolic,
            ),
            (
                "3 minutes",
                o.standing_3min_systolic,
                o.standing_3min_diastolic,
            ),
        ] {
            let sd = o.lying_systolic.zip(s).map(|(l, x)| l - x);
            let dd = o.lying_diastolic.zip(d).map(|(l, x)| l - x);
            if sd.is_some_and(|v| v >= c.orthostatic.systolic_drop)
                || dd.is_some_and(|v| v >= c.orthostatic.diastolic_drop)
            {
                abnormal = true;
            }
            if sd.is_some() || dd.is_some() {
                drops.push(format!(
                    "{label}: systolic drop {}, diastolic drop {}",
                    sd.map(|v| format!("{v:.0}")).unwrap_or_else(|| "—".into()),
                    dd.map(|v| format!("{v:.0}")).unwrap_or_else(|| "—".into())
                ));
            }
        }
        if drops.is_empty() && !o.symptoms {
            return Err(PhnError::InvalidInput(
                "give lying and standing blood pressures (or symptoms) for the orthostatic check"
                    .into(),
            ));
        }
        if abnormal {
            any_test_risk = true;
            add(
                "orthostatic",
                format!(
                    "Orthostatic blood pressure abnormal ({})",
                    if drops.is_empty() {
                        "symptoms on standing".to_string()
                    } else {
                        drops.join("; ")
                    }
                ),
            );
        }
        tests.push(TestOut {
            test: "Orthostatic blood pressure".into(),
            value: if drops.is_empty() {
                "symptoms only".into()
            } else {
                drops.join("; ")
            },
            threshold: format!(
                "systolic drop ≥{} or diastolic drop ≥{} mm Hg, or lightheadedness/dizziness",
                c.orthostatic.systolic_drop, c.orthostatic.diastolic_drop
            ),
            result: if abnormal { "at_risk" } else { "not_at_risk" }.into(),
            interpretation: c.orthostatic.rule.clone(),
            citation: cite(&c.orthostatic.source, &c.orthostatic.section),
        });
    }
    let mut unknown_factors = Vec::new();
    for f in &args.other_risk_factors {
        match normalize_factor(f) {
            Some(k) => add(k, f.clone()),
            None => unknown_factors.push(f.clone()),
        }
    }

    // ---- home hazards
    let mut home_safety = Vec::new();
    let mut unmatched_hazards = Vec::new();
    let want_all = args.home_hazards.iter().any(|h| {
        matches!(
            phn_core::text::normalize(h).as_str(),
            "all" | "checklist" | "full checklist"
        )
    });
    for area in &c.home_checklist.areas {
        for chk in &area.checks {
            if want_all {
                home_safety.push(HazardOut {
                    area: area.area.clone(),
                    hazard: chk.hazard.clone(),
                    fix: chk.fix.clone(),
                    matched_from: String::new(),
                });
            }
        }
    }
    if !want_all {
        for h in &args.home_hazards {
            let q = phn_core::text::normalize(h);
            let best = c
                .home_checklist
                .areas
                .iter()
                .flat_map(|a| a.checks.iter().map(move |k| (a, k)))
                .map(|(a, k)| {
                    let key = phn_core::text::normalize(&k.hazard);
                    let words: Vec<&str> = q.split(' ').filter(|w| w.len() > 3).collect();
                    let hits = words.iter().filter(|w| key.contains(*w)).count();
                    let score = if words.is_empty() {
                        0.0
                    } else {
                        hits as f64 / words.len() as f64
                    }
                    .max(phn_core::text::similarity(&q, &key));
                    (a, k, score)
                })
                .max_by(|x, y| x.2.total_cmp(&y.2));
            match best {
                Some((a, k, s)) if s >= 0.5 => home_safety.push(HazardOut {
                    area: a.area.clone(),
                    hazard: k.hazard.clone(),
                    fix: k.fix.clone(),
                    matched_from: h.clone(),
                }),
                _ => unmatched_hazards.push(h.clone()),
            }
        }
    }
    if !args.home_hazards.is_empty() && !want_all {
        add(
            "home_hazards",
            format!("Home hazards found: {}", args.home_hazards.join(", ")),
        );
    }

    if screen.at_risk.is_none() && tests.is_empty() && factors.is_empty() {
        return Err(PhnError::InvalidInput(
            "give the Stay Independent answers (12 yes/no), the three key questions (unsteady, worries_about_falling, fell_past_year), or at least one test result (tug_seconds, chair_stands, balance_seconds, orthostatic)".into(),
        ));
    }

    // ---- overall and plan
    let at_risk = screen.at_risk == Some(true) || any_test_risk;
    let overall = if at_risk {
        "at_risk"
    } else if screen.at_risk == Some(false) {
        "not_at_risk"
    } else {
        "incomplete"
    };
    let intervention_plan: Vec<InterventionOut> = c
        .interventions
        .iter()
        .filter(|i| factors.contains_key(&i.factor))
        .map(|i| InterventionOut {
            factor: factor_label(&i.factor).into(),
            finding: i.finding.clone(),
            actions: i.actions.clone(),
        })
        .collect();
    let gait_tested = tests.iter().any(|t| t.test != "Orthostatic blood pressure");
    let assessments_to_do: Vec<String> = if at_risk {
        c.assessments
            .iter()
            .filter(|a| match a.factor.as_str() {
                "gait_balance" => !gait_tested,
                "orthostatic" => args.orthostatic.is_none(),
                f => !factors.contains_key(f),
            })
            .map(|a| a.assess.clone())
            .collect()
    } else {
        Vec::new()
    };
    let prevention = if at_risk {
        Vec::new()
    } else {
        c.not_at_risk.clone()
    };
    let follow_up = if at_risk {
        c.at_risk_follow_up.trim().to_string()
    } else {
        format!(
            "{} ({})",
            c.screen_frequency.text,
            cite(&c.screen_frequency.source, &c.screen_frequency.section)
        )
    };
    let risk_factors: Vec<FactorOut> = factors
        .iter()
        .map(|(k, v)| FactorOut {
            factor: factor_label(k).into(),
            evidence: v.clone(),
        })
        .collect();

    // ---- markdown
    let mut s = format!(
        "**STEADI fall risk — {}**\n\n",
        match overall {
            "at_risk" => "AT RISK for falls",
            "not_at_risk" => "not at risk on screening",
            _ => "screen incomplete",
        }
    );
    s.push_str(&format!(
        "**Screen ({}):** {}{}\n",
        match screen.method.as_str() {
            "stay_independent" => "Stay Independent checklist",
            "key_questions" => "three key questions",
            _ => "not done",
        },
        screen
            .score
            .map(|v| format!("score {v} of {}. ", screen.max_score.unwrap_or(14)))
            .unwrap_or_default(),
        screen.reason
    ));
    if !tests.is_empty() {
        s.push('\n');
        let rows: Vec<Vec<String>> = tests
            .iter()
            .map(|t| {
                vec![
                    t.test.clone(),
                    t.value.clone(),
                    t.threshold.clone(),
                    t.result.replace('_', " "),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["Test", "Result", "CDC threshold", "Finding"],
            &rows,
        ));
    }
    if !intervention_plan.is_empty() {
        s.push_str("\n**Intervention plan (STEADI step 3)**\n");
        for i in &intervention_plan {
            s.push_str(&format!(
                "- **{}** ({}): {}\n",
                i.factor,
                i.finding,
                i.actions.join("; ")
            ));
        }
    }
    if !assessments_to_do.is_empty() {
        s.push_str("\n**Still to assess (STEADI step 2)**\n");
        for a in &assessments_to_do {
            s.push_str(&format!("- {a}\n"));
        }
    }
    if !prevention.is_empty() {
        s.push_str("\n**Prevention**\n");
        for p in &prevention {
            s.push_str(&format!("- {p}\n"));
        }
    }
    if !home_safety.is_empty() {
        s.push_str("\n**Home safety fixes (CDC Check for Safety)**\n");
        for h in &home_safety {
            s.push_str(&format!("- {}: {} → {}\n", h.area, h.hazard, h.fix));
        }
    }
    s.push_str(&format!("\n**Follow-up:** {follow_up}\n"));
    s.push_str("\nFind local evidence-based fall prevention programs (for example through the Council on Aging or ASAP) with find_ma_aging_services.\n");

    // ---- optional plan document
    let mut plan_file = None;
    let mut files_written = Vec::new();
    if let Some(fmt) = args
        .plan_format
        .as_deref()
        .map(|f| f.trim().to_ascii_lowercase())
        .filter(|f| !f.is_empty() && f != "none")
    {
        if fmt != "docx" && fmt != "pdf" {
            return Err(PhnError::InvalidInput(
                "plan_format must be \"docx\" or \"pdf\"".into(),
            ));
        }
        let doc = plan_document(
            ctx,
            &screen,
            &tests,
            &intervention_plan,
            &assessments_to_do,
            &prevention,
            &follow_up,
            &home_safety,
        );
        let bytes = if fmt == "docx" {
            phn_docs::docx::to_docx(&doc)?
        } else {
            phn_docs::pdf::to_pdf(&doc)?
        };
        let sandbox = ctx.sandbox()?;
        let path = sandbox.new_output_file(&format!("fall_prevention_plan.{fmt}"))?;
        std::fs::write(&path, bytes)?;
        let disp = sandbox.display_path(&path);
        s.push_str(&format!(
            "\nPlan written to `{disp}` (add the person's name there, not here).\n"
        ));
        files_written.push(disp.clone());
        plan_file = Some(disp);
    }

    let mut sources = vec![c10::content_source(FALLS)?];
    let mut keys: Vec<&str> = vec!["steadi-algorithm"];
    if screen.method == "stay_independent" {
        keys.push("stay-independent");
    }
    for t in &tests {
        keys.push(match t.test.as_str() {
            "Timed Up and Go" => "tug",
            "30-Second Chair Stand" => "chair-stand",
            "4-Stage Balance Test" => "four-stage",
            _ => "orthostatic",
        });
    }
    if !home_safety.is_empty() {
        keys.push(c.home_checklist.source.as_str());
    }
    keys.push(c.patient_actions_source.as_str());
    sources.extend(c10::sources_for(&c.sources, keys));
    sources.push(Source::computed(
        "Scores and comparisons calculated from the values given",
        "PHN MCP server",
    ));
    let mut caveats = vec![
        "STEADI's algorithm is written for community-dwelling adults 65 and older; use clinical judgment for younger adults and in facilities.".to_string(),
        "The tests support, but do not replace, a clinical fall risk evaluation; medication review needs a prescriber or pharmacist.".to_string(),
    ];
    if !unknown_factors.is_empty() {
        caveats.push(format!(
            "Risk factors not recognized (not used in the plan): {}.",
            unknown_factors.join(", ")
        ));
    }
    if !unmatched_hazards.is_empty() {
        caveats.push(format!(
            "Home hazards not matched to the CDC checklist: {}.",
            unmatched_hazards.join(", ")
        ));
    }
    caveats.extend(c10::review_caveats(&[FALLS]));
    Ok(Envelope::new(
        s,
        FallRiskResult {
            screen,
            tests,
            overall: overall.into(),
            risk_factors,
            intervention_plan,
            assessments_to_do,
            prevention,
            follow_up,
            home_safety,
            unmatched_hazards,
            patient_actions: c.patient_actions.clone(),
            test_procedures: procedures(),
            plan_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        files_written,
        notes: vec![
            "Only answers and test values were used; no names or identifiers are accepted.".into(),
        ],
        ..Privacy::none()
    })
    .decision_support())
}

fn procedures() -> Vec<ProcedureOut> {
    let c = &*CONTENT;
    vec![
        ProcedureOut {
            test: "Timed Up and Go".into(),
            procedure: format!(
                "{} Watch for: {}.",
                c.tug.procedure.trim(),
                c.tug.observations.join(", ").to_lowercase()
            ),
            citation: cite(&c.tug.source, &c.tug.section),
        },
        ProcedureOut {
            test: "30-Second Chair Stand".into(),
            procedure: c.chair_stand.procedure.trim().to_string(),
            citation: cite(&c.chair_stand.source, &c.chair_stand.section),
        },
        ProcedureOut {
            test: "4-Stage Balance Test".into(),
            procedure: format!(
                "Positions: {}. {}",
                c.four_stage
                    .stages
                    .iter()
                    .map(|st| format!("{}. {}", st.n, st.name))
                    .collect::<Vec<_>>()
                    .join("; "),
                c.four_stage.rule.trim()
            ),
            citation: cite(&c.four_stage.source, &c.four_stage.section),
        },
        ProcedureOut {
            test: "Orthostatic blood pressure".into(),
            procedure: c.orthostatic.procedure.trim().to_string(),
            citation: cite(&c.orthostatic.source, &c.orthostatic.section),
        },
    ]
}

#[allow(clippy::too_many_arguments)]
fn plan_document(
    ctx: &Context,
    screen: &ScreenOut,
    tests: &[TestOut],
    plan: &[InterventionOut],
    todo: &[String],
    prevention: &[String],
    follow_up: &str,
    home: &[HazardOut],
) -> Document {
    let c = &*CONTENT;
    let p = |t: &str| Block::Paragraph(vec![Inline::plain(t)]);
    let list = |v: &[String]| Block::List {
        ordered: false,
        items: v.iter().map(|x| vec![Inline::plain(x.clone())]).collect(),
    };
    let mut blocks = vec![
        p(
            "Name: ______________________   Date: ____________   Completed by: ______________________",
        ),
        Block::Heading {
            level: 2,
            text: "Screening".into(),
        },
        p(&format!("{} {}", screen.reason, screen.citation)),
    ];
    if !tests.is_empty() {
        blocks.push(Block::Heading {
            level: 2,
            text: "Gait, strength, balance, and blood pressure".into(),
        });
        blocks.push(Block::Table(TableBlock {
            caption: Some("Test results".into()),
            headers: ["Test", "Result", "CDC threshold", "Finding"]
                .map(String::from)
                .to_vec(),
            rows: tests
                .iter()
                .map(|t| {
                    vec![
                        t.test.clone(),
                        t.value.clone(),
                        t.threshold.clone(),
                        t.result.replace('_', " "),
                    ]
                })
                .collect(),
        }));
    }
    if !plan.is_empty() {
        blocks.push(Block::Heading {
            level: 2,
            text: "Plan".into(),
        });
        blocks.push(Block::Table(TableBlock {
            caption: Some("Risk factors and interventions".into()),
            headers: [
                "Risk factor",
                "Finding",
                "Interventions",
                "Who / by when",
                "Done",
            ]
            .map(String::from)
            .to_vec(),
            rows: plan
                .iter()
                .map(|i| {
                    vec![
                        i.factor.clone(),
                        i.finding.clone(),
                        i.actions.join("; "),
                        String::new(),
                        String::new(),
                    ]
                })
                .collect(),
        }));
    }
    if !todo.is_empty() {
        blocks.push(Block::Heading {
            level: 2,
            text: "Still to assess".into(),
        });
        blocks.push(list(todo));
    }
    if !prevention.is_empty() {
        blocks.push(Block::Heading {
            level: 2,
            text: "Prevention".into(),
        });
        blocks.push(list(prevention));
    }
    blocks.push(Block::Heading {
        level: 2,
        text: "Home safety checklist".into(),
    });
    let rows: Vec<Vec<String>> = if home.is_empty() {
        c.home_checklist
            .areas
            .iter()
            .flat_map(|a| {
                a.checks.iter().map(move |k| {
                    vec![
                        a.area.clone(),
                        k.hazard.clone(),
                        k.fix.clone(),
                        String::new(),
                    ]
                })
            })
            .collect()
    } else {
        home.iter()
            .map(|h| {
                vec![
                    h.area.clone(),
                    h.hazard.clone(),
                    h.fix.clone(),
                    String::new(),
                ]
            })
            .collect()
    };
    blocks.push(Block::Table(TableBlock {
        caption: Some("Home hazards and fixes (CDC Check for Safety)".into()),
        headers: ["Area", "Hazard", "Fix", "Done"].map(String::from).to_vec(),
        rows,
    }));
    blocks.push(Block::Heading {
        level: 2,
        text: "What you can do".into(),
    });
    blocks.push(list(&c.patient_actions));
    blocks.push(Block::Heading {
        level: 2,
        text: "Follow-up".into(),
    });
    blocks.push(p(follow_up));
    blocks.push(p("Based on CDC STEADI (Stopping Elderly Accidents, Deaths & Injuries) materials. Decision support only; the plan is the clinician's."));
    Document {
        title: "Fall prevention plan".into(),
        subtitle: Some("CDC STEADI: screen, assess, intervene".into()),
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
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    #[test]
    fn content_sources_are_known() {
        let c = &*CONTENT;
        c10::assert_keys_known(
            &c.sources,
            &[
                &c.key_questions.source,
                &c.tug.source,
                &c.chair_stand.source,
                &c.four_stage.source,
                &c.orthostatic.source,
                &c.home_checklist.source,
                &c.screen_frequency.source,
                &c.patient_actions_source,
            ],
            "steadi",
        );
        assert_eq!(
            c.stay_independent
                .items
                .iter()
                .map(|i| i.points)
                .sum::<u32>(),
            14
        );
    }

    #[test]
    fn chair_stand_norms_follow_cdc_table() {
        assert_eq!(chair_stand_cutoff(60, false), Some(14));
        assert_eq!(chair_stand_cutoff(64, true), Some(12));
        assert_eq!(chair_stand_cutoff(72, true), Some(10));
        assert_eq!(chair_stand_cutoff(87, false), Some(8));
        assert_eq!(chair_stand_cutoff(94, true), Some(4));
        assert_eq!(chair_stand_cutoff(59, true), None);
        assert_eq!(chair_stand_cutoff(95, false), None);
    }

    #[test]
    fn screening_rules_and_plan() {
        let (_d, ctx) = ctx();
        // Score 3 without a fall: not at risk.
        let mut si = vec![Some(false); 12];
        si[2] = Some(true);
        si[4] = Some(true);
        si[7] = Some(true);
        let r = steadi_fall_risk_assessment(
            &ctx,
            FallRiskArgs {
                stay_independent: si.clone(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.screen.score, Some(3));
        assert_eq!(r.data.overall, "not_at_risk");
        assert!(!r.data.prevention.is_empty());
        // Item 1 (fell) alone scores 2 but means at risk.
        let mut fell = vec![Some(false); 12];
        fell[0] = Some(true);
        let r = steadi_fall_risk_assessment(
            &ctx,
            FallRiskArgs {
                stay_independent: fell,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.screen.score, Some(2));
        assert_eq!(r.data.overall, "at_risk");
        // Score 4 is at risk; tests add findings and a plan document is written.
        si[9] = Some(true);
        let r = steadi_fall_risk_assessment(
            &ctx,
            FallRiskArgs {
                stay_independent: si,
                age_years: Some(78),
                sex: Some("female".into()),
                tug_seconds: Some(13.5),
                chair_stands: Some(8),
                balance_seconds: vec![10.0, 10.0, 6.0],
                orthostatic: Some(OrthostaticInput {
                    lying_systolic: Some(140.0),
                    lying_diastolic: Some(80.0),
                    standing_1min_systolic: Some(118.0),
                    standing_1min_diastolic: Some(78.0),
                    ..Default::default()
                }),
                home_hazards: vec![
                    "throw rugs in the hallway".into(),
                    "no grab bars by the toilet".into(),
                ],
                plan_format: Some("docx".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.screen.score, Some(4));
        assert_eq!(r.data.overall, "at_risk");
        assert!(
            r.data.tests.iter().all(|t| t.result == "at_risk"),
            "{:?}",
            r.data.tests
        );
        let factors: Vec<&str> = r
            .data
            .intervention_plan
            .iter()
            .map(|i| i.factor.as_str())
            .collect();
        for f in [
            "gait and balance",
            "leg strength",
            "orthostatic hypotension",
            "medications",
            "home hazards",
        ] {
            assert!(factors.contains(&f), "{f} missing from {factors:?}");
        }
        assert_eq!(r.data.home_safety.len(), 2, "{:?}", r.data.home_safety);
        assert!(r.data.plan_file.is_some());
        assert_eq!(r.privacy.files_written.len(), 1);
        // Nothing to score is a tool error.
        assert!(steadi_fall_risk_assessment(&ctx, FallRiskArgs::default()).is_err());
        assert!(
            steadi_fall_risk_assessment(
                &ctx,
                FallRiskArgs {
                    tug_seconds: Some(0.0),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn tug_boundary_and_chair_stand_without_norms() {
        let (_d, ctx) = ctx();
        let r = |t: f64| {
            steadi_fall_risk_assessment(
                &ctx,
                FallRiskArgs {
                    tug_seconds: Some(t),
                    ..Default::default()
                },
            )
            .unwrap()
        };
        assert_eq!(r(11.9).data.tests[0].result, "not_at_risk");
        assert_eq!(r(12.0).data.tests[0].result, "at_risk");
        let x = steadi_fall_risk_assessment(
            &ctx,
            FallRiskArgs {
                chair_stands: Some(10),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(x.data.tests[0].result, "no_norm");
        assert_eq!(x.data.overall, "incomplete");
    }

    #[test]
    fn falls_count_answers_the_fall_question() {
        let (_d, ctx) = ctx();
        // Two falls is a yes to "fell in the past year": at risk.
        let x = steadi_fall_risk_assessment(
            &ctx,
            FallRiskArgs {
                unsteady: Some(false),
                worries_about_falling: Some(false),
                falls_count: Some(2),
                ..FallRiskArgs::default()
            },
        )
        .unwrap();
        assert_eq!(x.data.overall, "at_risk");
        // No falls is not a risk factor and adds no fall-history interventions.
        let x = steadi_fall_risk_assessment(
            &ctx,
            FallRiskArgs {
                unsteady: Some(false),
                worries_about_falling: Some(false),
                falls_count: Some(0),
                ..FallRiskArgs::default()
            },
        )
        .unwrap();
        assert_eq!(x.data.overall, "not_at_risk");
        assert!(x.data.risk_factors.is_empty());
        assert!(x.data.intervention_plan.is_empty());
        // A zero count alone is not a screen, and a contradiction is an error.
        assert!(
            steadi_fall_risk_assessment(
                &ctx,
                FallRiskArgs {
                    falls_count: Some(0),
                    ..FallRiskArgs::default()
                },
            )
            .is_err()
        );
        assert!(
            steadi_fall_risk_assessment(
                &ctx,
                FallRiskArgs {
                    fell_past_year: Some(false),
                    falls_count: Some(1),
                    ..FallRiskArgs::default()
                },
            )
            .is_err()
        );
    }
}
