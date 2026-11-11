//! SCH-05 `concussion_return_to_learn_tracker`.
//!
//! Applies 105 CMR 201.000 (removal from play, notification deadlines,
//! forms, graduated reentry plan, medical clearance) and DPH's return-to-learn
//! guidance and clearance-form stages, with CDC HEADS UP steps, to one
//! student's dates (no identifying details are taken) or to a local head
//! injury log. In log mode, per-student compliance goes to an XLSX in the
//! output folder, filled reentry plans and caregiver letters can be written
//! there, and only suppressed counts (including the 105 CMR 201.017 annual
//! statistics) are returned.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, NaiveDate};
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use phn_epi::suppression::{SuppressionOptions, suppress};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::localdata;
use crate::md;
use crate::school::calendar::{SchoolYear, add_business_days, next_weekday};
use crate::school::content::{self, CONCUSSION_INFO, CitedOut, confidence_suffix, outs};
use crate::school::screenings::{find_col, result_language_label};
use crate::school::templates::{self, DocFormat, LanguageCount, TemplateResolver};

/// Which return-to-school guidance the district follows for the first days.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RestGuidance {
    /// DPH 2018: rest 2-4 days; most students need 1-4 days off (default).
    #[default]
    Dph2018,
    /// CDC 2025: most children can return within 1-2 days.
    Cdc2025,
}

/// Arguments for `concussion_return_to_learn_tracker`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ConcussionArgs {
    /// Date of the head injury (YYYY-MM-DD). Do not give the student's name.
    pub injury_date: Option<String>,
    /// The injury happened in an extracurricular athletic activity (practice,
    /// game, marching band) at a school covered by 105 CMR 201.000.
    pub during_extracurricular: Option<bool>,
    /// Date a provider diagnosed a concussion, if diagnosed.
    pub diagnosis_date: Option<String>,
    /// First day back at school after the injury.
    pub returned_to_school: Option<String>,
    /// Date the student became symptom free at rest.
    pub symptom_free_date: Option<String>,
    /// Date the student was back in class full time without concussion-related
    /// academic accommodations.
    pub full_academics_date: Option<String>,
    /// Date a provider cleared the student to begin the gradual return to play.
    pub cleared_to_begin_rtp: Option<String>,
    /// Highest gradual return-to-play stage completed without symptoms (0-6,
    /// numbered as on DPH's clearance form).
    pub rtp_stage_completed: Option<u8>,
    /// Current symptoms in plain words (e.g. ["headache", "light sensitivity"]),
    /// to suggest accommodations.
    pub symptoms: Option<Vec<String>>,
    /// Ratings 0-6 from DPH's Post-concussion Symptom Monitoring Tool,
    /// symptom name -> rating.
    pub symptom_ratings: Option<BTreeMap<String, u8>>,
    /// Evaluation date (default today).
    pub as_of: Option<String>,
    /// DPH 2018 (default) or CDC 2025 guidance for time out of school.
    pub rest_guidance: Option<RestGuidance>,
    /// Log mode: a CSV/XLSX head injury log in /data (one row per injury) with
    /// columns such as Injury Date, Extracurricular, Removed From Play, Parent
    /// Notified, Written Notice, AD Nurse Notified, Head Injury Form, Diagnosed
    /// Concussion, Reentry Plan, Symptom Free, Full Academics, Clearance Date,
    /// Cleared By, RTP Stage, Full Return. Per-student results go to /data/output.
    pub tracker_path: Option<String>,
    /// Sheet name for an XLSX log.
    pub sheet: Option<String>,
    /// Log mode: also write a filled graduated reentry plan and a caregiver
    /// letter (in the family's language when the agency has a translation)
    /// for each diagnosed concussion (default false).
    pub write_plans: Option<bool>,
    /// docx (default) or pdf.
    pub format: Option<DocFormat>,
    /// School name for plans and letters.
    pub school_name: Option<String>,
    /// Your name for plans and letters.
    pub nurse_name: Option<String>,
    /// Your phone for plans and letters.
    pub nurse_phone: Option<String>,
}

/// A dated step or requirement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimelineItem {
    /// Due date or date done (YYYY-MM-DD), when known.
    pub date: Option<String>,
    pub what: String,
    /// done, due, overdue, upcoming, not_yet, or n/a.
    pub status: String,
    pub citation: String,
}

/// A stage of a return plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StageOut {
    pub id: String,
    pub name: String,
    /// Typical length in days, as DPH gives it.
    pub typical_days: Option<String>,
    pub text: String,
    pub activities: Vec<String>,
    /// For return to play: completed, next, or later.
    pub status: Option<String>,
    pub citation: String,
}

/// Suggested accommodations for one post-concussion effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AccommodationOut {
    pub effect: String,
    pub problem: String,
    pub supports: Vec<String>,
    pub citation: String,
}

/// A required form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FormOut {
    pub name: String,
    pub who_completes: String,
    pub when: String,
    pub url: String,
    pub translations: Vec<String>,
    pub citation: String,
}

/// Symptom score from DPH's monitoring tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SymptomScore {
    pub total: u32,
    pub max_possible: u32,
    pub symptoms_present: usize,
    /// Names that did not match DPH's 22 symptoms (ignored).
    pub unrecognized: Vec<String>,
}

/// Suppressed counts from a head injury log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LogSummary {
    pub file: String,
    pub school_year: String,
    pub injuries: usize,
    /// 105 CMR 201.017(A): Report of Head Injury forms received ("*" when suppressed).
    pub head_injury_forms: String,
    /// 105 CMR 201.017(B): students with head injuries or suspected concussions
    /// in extracurricular athletics.
    pub students_extracurricular: String,
    pub diagnosed_concussions: String,
    pub open_cases: String,
    /// Check -> number of injuries not meeting it ("*" when suppressed).
    pub gaps: BTreeMap<String, String>,
    /// Notices marked done without a date (written notice to the parent, or telling the
    /// athletic director and nurse), so the next-business-day deadline could not be checked;
    /// they are not counted as gaps ("*" when suppressed; absent when there are none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notices_without_date: Option<String>,
    pub output_file: String,
    pub suppression_threshold: u32,
}

/// Plans and letters written in log mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlansWritten {
    pub folder: String,
    pub plans: usize,
    pub letters: usize,
    pub files: Vec<String>,
    pub by_language: Vec<LanguageCount>,
    pub english_fallback: usize,
    /// Placeholders in an agency template that the tool does not fill.
    pub unknown_placeholders: Vec<String>,
}

/// Result of `concussion_return_to_learn_tracker`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConcussionResult {
    pub evaluated_on: String,
    pub rest_guidance: RestGuidance,
    pub timeline: Vec<TimelineItem>,
    pub flags: Vec<String>,
    pub return_to_learn: Vec<StageOut>,
    pub return_to_play: Vec<StageOut>,
    pub accommodations: Vec<AccommodationOut>,
    pub symptom_score: Option<SymptomScore>,
    pub monitoring_schedule: Vec<String>,
    pub forms: Vec<FormOut>,
    pub danger_signs: Vec<CitedOut>,
    pub reentry_plan_requirements: Vec<CitedOut>,
    pub clearance_rules: Vec<CitedOut>,
    pub divergences: Vec<CitedOut>,
    /// Template placeholders for the reentry plan and caregiver letter.
    pub template_placeholders: Vec<String>,
    pub log: Option<LogSummary>,
    pub plans: Option<PlansWritten>,
}

// ------------------------------------------------------------------ helpers

fn date_arg(field: &str, v: &Option<String>) -> Result<Option<NaiveDate>> {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => crate::vax::dates::require_date(field, s).map(Some),
        None => Ok(None),
    }
}

fn cite(key: &str, section: &str) -> String {
    content::citation(&CONCUSSION_INFO.sources, key, section)
}

fn status_for(done: Option<NaiveDate>, due: NaiveDate, as_of: NaiveDate) -> String {
    match done {
        Some(d) if d <= due => "done".into(),
        Some(_) => "done late".into(),
        None if as_of > due => "overdue".into(),
        None if as_of == due => "due today".into(),
        None => "upcoming".into(),
    }
}

/// Accommodations whose effect matches the symptoms (all when none given).
fn accommodations_for(symptoms: &[String]) -> Vec<AccommodationOut> {
    let c = &*CONCUSSION_INFO;
    let map: &[(&str, &[&str])] = &[
        (
            "Attention",
            &["attention", "concentrat", "focus", "distract"],
        ),
        (
            "Working memory",
            &["working memory", "instructions", "reading", "math"],
        ),
        ("Memory", &["memory", "remember", "forget"]),
        ("Processing speed", &["slow", "processing", "pace"]),
        ("Fatigue", &["fatigue", "tired", "energy", "drows"]),
        ("Headaches", &["headache", "head pain", "pressure"]),
        (
            "Light",
            &["light", "noise", "sound", "screen", "vision", "blur"],
        ),
        ("Dizziness", &["dizz", "balance", "unsteady"]),
        ("Sleep", &["sleep", "insomnia"]),
        ("Anxiety", &["anxi", "nervous", "worry"]),
        ("Symptom sensitivity", &["worse with activity", "overactiv"]),
        (
            "Emotional",
            &["sad", "irritab", "mood", "emotional", "angry", "depress"],
        ),
    ];
    let wanted: BTreeSet<&str> = if symptoms.is_empty() {
        map.iter().map(|(k, _)| *k).collect()
    } else {
        let lower: Vec<String> = symptoms.iter().map(|s| s.to_lowercase()).collect();
        map.iter()
            .filter(|(_, ws)| lower.iter().any(|s| ws.iter().any(|w| s.contains(w))))
            .map(|(k, _)| *k)
            .collect()
    };
    c.accommodations
        .iter()
        .filter(|a| wanted.iter().any(|w| a.effect.starts_with(w)))
        .map(|a| AccommodationOut {
            effect: a.effect.clone(),
            problem: a.problem.clone(),
            supports: a.supports.clone(),
            citation: cite(&a.source, &a.section),
        })
        .collect()
}

fn score(ratings: &BTreeMap<String, u8>) -> Result<SymptomScore> {
    let list = &CONCUSSION_INFO.symptom_monitoring.symptoms;
    let mut total = 0u32;
    let mut present = 0usize;
    let mut unrecognized = Vec::new();
    for (k, v) in ratings {
        if *v > 6 {
            return Err(PhnError::InvalidInput(format!(
                "symptom rating for '{k}' must be 0-6 (DPH's scale)"
            )));
        }
        let kn = phn_core::text::normalize(k);
        let hit = list.iter().any(|s| {
            let sn = phn_core::text::normalize(s);
            sn == kn || phn_core::text::similarity(&kn, &sn) >= 0.75 || sn.contains(&kn)
        });
        if !hit {
            unrecognized.push(k.clone());
            continue;
        }
        total += u32::from(*v);
        if *v > 0 {
            present += 1;
        }
    }
    Ok(SymptomScore {
        total,
        max_possible: 6 * list.len() as u32,
        symptoms_present: present,
        unrecognized,
    })
}

fn stage_out(s: &crate::school::content::Stage, status: Option<String>) -> StageOut {
    StageOut {
        id: s.id.clone(),
        name: s.name.clone(),
        typical_days: match (s.min_days, s.max_days) {
            (Some(a), Some(b)) => Some(format!("{a}-{b} days")),
            (Some(0), None) => Some("when symptoms are gone".into()),
            (Some(a), None) => Some(format!("{a}+ days")),
            _ => None,
        },
        text: s.text.trim().to_string(),
        activities: s.activities.clone(),
        status,
        citation: cite(&s.source, &s.section),
    }
}

// ------------------------------------------------------------------ templates

/// Placeholders the reentry plan and caregiver letter use.
pub const PLAN_PLACEHOLDERS: &[&str] = &[
    "student_name",
    "parent_name",
    "grade",
    "school",
    "sport",
    "injury_date",
    "diagnosis_date",
    "nurse_name",
    "nurse_phone",
    "today",
];

/// The graduated reentry plan template (Markdown with placeholders).
pub fn reentry_plan_template() -> String {
    let c = &*CONCUSSION_INFO;
    let mut s = String::from(
        "# Graduated reentry plan after concussion\n\n\
         | | |\n|---|---|\n\
         | Student | {{student_name}} |\n| Grade | {{grade}} |\n| School | {{school}} |\n\
         | Activity | {{sport}} |\n| Date of injury | {{injury_date}} |\n\
         | Date diagnosed | {{diagnosis_date}} |\n| Point person | {{nurse_name}}, {{nurse_phone}} |\n\
         | Plan written | {{today}} |\n\n\
         Required by 105 CMR 201.010(E) for every student removed from play and diagnosed with a \
         concussion. Developed by the student's teachers, guidance counselor, school nurse, \
         athletic trainer if on staff, neuropsychologist if involved, and parent, in consultation \
         with the student's health care provider.\n\n## 1. Rest and return to classes\n\n",
    );
    s.push_str(
        "| Stage | Typical length | What the student does | Date started |\n|---|---|---|---|\n",
    );
    for st in &c.return_to_learn {
        let o = stage_out(st, None);
        s.push_str(&format!(
            "| {} | {} | {} | |\n",
            o.name,
            o.typical_days.unwrap_or_default(),
            o.text.replace('|', "/")
        ));
    }
    s.push_str("\n## 2. Accommodations (check those that apply)\n\n");
    for a in &c.accommodations {
        s.push_str(&format!(
            "- [ ] **{}**: {}\n",
            a.effect,
            a.supports.join("; ")
        ));
    }
    s.push_str(&format!(
        "\n## 3. Symptom checks\n\nThe school nurse rates the 22 symptoms on DPH's Post-concussion \
         Symptom Monitoring Tool ({}): {}\n\n## 4. Return to athletics\n\nThe student must be \
         symptom free at rest to begin, and back in class full time without concussion-related \
         accommodations before a provider clears the start of the gradual return to play (DPH \
         clearance form). Each stage takes at least 24 hours; if symptoms return, go back to the \
         previous stage.\n\n| Stage | Date completed | Initials |\n|---|---|---|\n",
        c.symptom_monitoring.scale,
        c.symptom_monitoring.frequency.trim()
    ));
    for st in &c.return_to_play_dph {
        s.push_str(&format!("| {} | | |\n", st.name));
    }
    s.push_str(
        "\nMedical clearance: Post Sports-Related Head Injury Medical Clearance and Authorization \
         Form, completed by an authorized provider (105 CMR 201.011), given to the athletic director.\n\n\
         ## 5. Communication\n\nThe point person shares updates among teachers, the guidance \
         counselor, the athletic trainer, the family, and the student's provider. If the student \
         has not returned to a full academic load within 3 to 4 weeks, arrange a comprehensive \
         assessment and consider a concussion management team and a Section 504 or IEP review.\n\n\
         ## Signatures\n\nParent or guardian: ____________________ Date: ________\n\n\
         School nurse: ____________________ Date: ________\n",
    );
    s
}

/// Caregiver letter adapted from DPH's Sample Letter 1 (Returning to School After Concussion, 2018).
pub const CAREGIVER_LETTER: &str = "# Your child's concussion: plan for returning to school

{{today}}

Dear {{parent_name}},

On {{diagnosis_date}}, {{student_name}} (grade {{grade}}) was diagnosed with a concussion. Based on the symptoms and the doctor's orders, the school will support your child with temporary accommodations, which may include shorter days, access to the health office, no physical education or physical activity, extra time for class work, less homework, and no standardized tests until symptoms are gone.

We ask that you:

- Send us any notes from medical appointments so we can adjust the accommodations.
- Call the school if your child will be late, dismissed early, or absent because of the concussion.

Massachusetts rules (105 CMR 201.000) require written medical clearance before a student returns to extracurricular athletics, and athletes complete a gradual return to play; we use DPH's Medical Clearance and Authorization Form for this. Please also ask your child's provider when your child may return to physical education and a full academic workload.

If you have questions, please contact me.

{{nurse_name}}, School Nurse
{{nurse_phone}}
{{school}}
";

// ------------------------------------------------------------------ single student

/// Timeline, flags, return-to-play stages, and symptom-check dates.
type SingleOut = (Vec<TimelineItem>, Vec<String>, Vec<StageOut>, Vec<String>);

/// Log summary, plans, files written, file label, and identifier categories.
type LogOut = (
    LogSummary,
    Option<PlansWritten>,
    Vec<String>,
    String,
    Vec<String>,
);

fn single(args: &ConcussionArgs, as_of: NaiveDate) -> Result<SingleOut> {
    let c = &*CONCUSSION_INFO;
    let injury = date_arg("injury_date", &args.injury_date)?;
    let diagnosis = date_arg("diagnosis_date", &args.diagnosis_date)?;
    let back = date_arg("returned_to_school", &args.returned_to_school)?;
    let symptom_free = date_arg("symptom_free_date", &args.symptom_free_date)?;
    let full_acad = date_arg("full_academics_date", &args.full_academics_date)?;
    let cleared = date_arg("cleared_to_begin_rtp", &args.cleared_to_begin_rtp)?;
    for (name, d) in [
        ("diagnosis_date", diagnosis),
        ("returned_to_school", back),
        ("symptom_free_date", symptom_free),
        ("full_academics_date", full_acad),
        ("cleared_to_begin_rtp", cleared),
    ] {
        if let (Some(i), Some(x)) = (injury, d)
            && x < i
        {
            return Err(PhnError::InvalidInput(format!(
                "{name} ({x}) is before the injury date ({i})"
            )));
        }
    }
    if let Some(i) = injury
        && i > as_of
    {
        return Err(PhnError::InvalidInput(
            "the injury date is after the evaluation date".into(),
        ));
    }
    let stage = args.rtp_stage_completed.unwrap_or(0);
    if stage > 6 {
        return Err(PhnError::InvalidInput(
            "rtp_stage_completed must be 0-6".into(),
        ));
    }
    let extracurricular = args.during_extracurricular.unwrap_or(true);
    let mut t = Vec::new();
    let mut flags = Vec::new();
    if let Some(i) = injury {
        if extracurricular {
            t.push(TimelineItem {
                date: Some(i.to_string()),
                what: "Remove from practice or competition immediately; no return that day".into(),
                status: "required".into(),
                citation: cite("cmr201", "201.010(A)"),
            });
            t.push(TimelineItem {
                date: Some(i.to_string()),
                what:
                    "Coach tells the parent in person or by phone right after the practice or game"
                        .into(),
                status: if as_of > i {
                    "should be done".into()
                } else {
                    "due today".into()
                },
                citation: cite("cmr201", "201.010(C)"),
            });
            let nbd = add_business_days(i, 1);
            t.push(TimelineItem {
                date: Some(nbd.to_string()),
                what: "Written notice to the parent (paper or electronic) by end of the next business day".into(),
                status: status_for(None, nbd, as_of).replace("overdue", "check: overdue if not done"),
                citation: cite("cmr201", "201.010(C)"),
            });
            t.push(TimelineItem {
                date: Some(nbd.to_string()),
                what: "Coach or designee tells the athletic director and school nurse by end of the next business day".into(),
                status: status_for(None, nbd, as_of).replace("overdue", "check: overdue if not done"),
                citation: cite("cmr201", "201.010(D)"),
            });
            t.push(TimelineItem {
                date: None,
                what: "Coach completes the Report of a Head Injury During Sports Season form and sends it to the school nurse for review".into(),
                status: "required".into(),
                citation: cite("cmr201", "201.013(A)(3), (A)(5); 201.015(D)"),
            });
        } else {
            t.push(TimelineItem {
                date: None,
                what: "Injury outside the activity during a sports season: the parent completes the Report of a Head Injury form for the coach or school nurse".into(),
                status: "required if the student is an athlete in season".into(),
                citation: cite("cmr201", "201.008(B)"),
            });
        }
        let four = i.checked_add_signed(Duration::days(28)).unwrap_or(i);
        let not_back = full_acad.is_none_or(|d| d > four);
        t.push(TimelineItem {
            date: Some(four.to_string()),
            what: "4 weeks after injury: if not back to a full academic load, arrange a comprehensive multidisciplinary assessment and consider a concussion management team and 504/IEP review".into(),
            status: if !not_back {
                "n/a (full academics resumed earlier)".into()
            } else if as_of >= four {
                "reached".into()
            } else {
                "upcoming".into()
            },
            citation: cite("dph-rtl-2018", "Highlights; When Symptoms Persist"),
        });
        if not_back && as_of >= four {
            flags.push(
                "More than 4 weeks since injury without a documented return to a full academic load: DPH advises a comprehensive multidisciplinary assessment, a more formal concussion management team, and considering a Section 504 plan or IEP.".into(),
            );
        }
    }
    if diagnosis.is_some() || injury.is_some() {
        t.push(TimelineItem {
            date: diagnosis.map(|d| d.to_string()),
            what: "Written graduated reentry plan (required for a diagnosed concussion), coordinated by a point person".into(),
            status: if diagnosis.is_some() { "required now".into() } else { "required if diagnosed".into() },
            citation: cite("cmr201", "201.010(E)"),
        });
    }
    // Return to play gating.
    if let (Some(c0), Some(fa)) = (cleared, full_acad)
        && c0 < fa
    {
        flags.push(format!(
            "The provider cleared the start of return to play ({c0}) before the student was back in class full time without accommodations ({fa}); DPH's clearance form says not to clear until then."
        ));
    }
    if let (Some(c0), None) = (cleared, full_acad) {
        flags.push(format!(
            "Clearance to begin return to play is recorded ({c0}) but no date of full-time classes without accommodations; confirm before starting (DPH clearance form)."
        ));
    }
    if stage > 0 && symptom_free.is_none() {
        flags.push(
            "Return-to-play stages are recorded but no symptom-free date: the student must be completely symptom free at rest to begin (105 CMR 201.010(E)(3)).".into(),
        );
    }
    if stage > 0 && cleared.is_none() {
        flags.push(
            "Return-to-play stages are recorded without a provider's clearance to begin (DPH Medical Clearance and Authorization Form).".into(),
        );
    }
    let rtp: Vec<StageOut> = c
        .return_to_play_dph
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let n = i as u8 + 1;
            let status = if n <= stage {
                "completed"
            } else if n == stage + 1 {
                "next"
            } else {
                "later"
            };
            stage_out(s, Some(status.into()))
        })
        .collect();
    if stage < 6 {
        let ready = symptom_free.is_some() && full_acad.is_some() && cleared.is_some();
        let next = as_of.succ_opt().unwrap_or(as_of);
        t.push(TimelineItem {
            date: if ready || stage > 0 { Some(next.to_string()) } else { None },
            what: format!(
                "Return to play: next stage {} ({}){}",
                stage + 1,
                c.return_to_play_dph[stage as usize].name,
                if stage + 1 == 5 {
                    "; the provider signs the clearance form before full contact"
                } else {
                    ""
                }
            ),
            status: if ready || stage > 0 {
                "earliest, if no symptoms in the current stage (each stage at least 24 hours)".into()
            } else {
                "not yet: needs symptom free at rest, full-time classes without accommodations, and provider clearance".into()
            },
            citation: cite("dph-clearance-form", "Gradual Return to Play Protocol"),
        });
    }
    // Symptom monitoring schedule from the return to school.
    let mut schedule = Vec::new();
    if let Some(b) = back {
        for d in [0i64, 3, 7, 10, 14, 21, 28] {
            // Checks happen at school: a Saturday or Sunday moves to Monday.
            let Some(day) = b.checked_add_signed(Duration::days(d)) else {
                break;
            };
            let day = next_weekday(day).to_string();
            if schedule.last() != Some(&day) {
                schedule.push(day);
            }
        }
    }
    Ok((t, flags, rtp, schedule))
}

// ------------------------------------------------------------------ log mode

fn cellv(row: &[String], i: Option<usize>) -> &str {
    i.and_then(|i| row.get(i))
        .map(String::as_str)
        .unwrap_or("")
        .trim()
}

fn yes(s: &str) -> bool {
    localdata::truthy(s) == Some(true) || crate::vax::dates::parse_date(s).is_some()
}

fn col(h: &[String], names: &[&str]) -> Option<usize> {
    find_col(h, names).or_else(|| {
        h.iter().position(|x| {
            let n = phn_core::text::normalize(x);
            names.iter().any(|k| n.contains(k))
        })
    })
}

struct LogCols {
    injury: usize,
    extracurricular: Option<usize>,
    removed: Option<usize>,
    returned_same_day: Option<usize>,
    written_notice: Option<usize>,
    ad_nurse: Option<usize>,
    form: Option<usize>,
    diagnosed: Option<usize>,
    diagnosis_date: Option<usize>,
    plan: Option<usize>,
    symptom_free: Option<usize>,
    full_academics: Option<usize>,
    clearance: Option<usize>,
    cleared_by: Option<usize>,
    full_return: Option<usize>,
    grade: Option<usize>,
    sport: Option<usize>,
    language: Option<usize>,
    student_name: Option<usize>,
    first: Option<usize>,
    last: Option<usize>,
    parent: Option<usize>,
}

fn log_cols(h: &[String]) -> Result<LogCols> {
    let injury = col(h, &["injury date", "date of injury", "injury"]).ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "the log needs an 'Injury Date' column. Columns found: {}",
            h.join(", ")
        ))
    })?;
    Ok(LogCols {
        injury,
        extracurricular: col(
            h,
            &[
                "extracurricular",
                "during sport",
                "sport related",
                "athletic activity",
            ],
        ),
        removed: col(h, &["removed from play", "removed"]),
        returned_same_day: col(h, &["returned same day", "same day return"]),
        written_notice: col(
            h,
            &["written notice", "parent written", "written notification"],
        ),
        ad_nurse: col(
            h,
            &[
                "ad nurse notified",
                "nurse notified",
                "ad notified",
                "athletic director notified",
            ],
        ),
        form: col(h, &["head injury form", "report of head injury", "rohi"]),
        diagnosed: col(h, &["diagnosed concussion", "diagnosed", "concussion"]),
        diagnosis_date: col(h, &["diagnosis date", "date diagnosed"]),
        plan: col(
            h,
            &[
                "reentry plan",
                "re entry plan",
                "return to learn plan",
                "plan date",
            ],
        ),
        symptom_free: col(h, &["symptom free"]),
        full_academics: col(
            h,
            &["full academics", "full time classes", "no accommodations"],
        ),
        clearance: col(
            h,
            &[
                "clearance date",
                "cleared date",
                "medical clearance",
                "clearance",
            ],
        ),
        cleared_by: col(h, &["cleared by", "clearing provider", "provider type"]),
        full_return: col(
            h,
            &["full return", "return to competition", "full activity"],
        ),
        grade: find_col(h, &["grade", "grade level"]),
        sport: col(h, &["sport", "activity", "team"]),
        // Exact header names only (a column that merely contains "language" is not it).
        language: find_col(
            h,
            &[
                "home language",
                "primary language",
                "family language",
                "preferred language",
                "language",
            ],
        ),
        student_name: find_col(h, &["student name", "student", "name"]),
        first: find_col(h, &["first name", "first"]),
        last: find_col(h, &["last name", "last", "surname"]),
        parent: col(h, &["parent name", "guardian name", "parent", "guardian"]),
    })
}

const ALLOWED_CLEARERS: &[&str] = &[
    "physician",
    "md",
    "do",
    "doctor",
    "athletic trainer",
    "atc",
    "lat",
    "nurse practitioner",
    "np",
    "physician assistant",
    "pa",
    "neuropsychologist",
];

const CHECKS: [&str; 7] = [
    "written notice by next business day",
    "AD and nurse told by next business day",
    "Report of Head Injury form on file",
    "reentry plan for diagnosed concussion",
    "clearance before return",
    "clearance after full academics",
    "clearing provider type allowed",
];

#[allow(clippy::too_many_lines)]
fn log_mode(ctx: &Context, args: &ConcussionArgs, as_of: NaiveDate) -> Result<LogOut> {
    let path = args.tracker_path.as_deref().unwrap_or_default();
    let lt = localdata::load(ctx, path, args.sheet.as_deref())?;
    let h = lt.table.headers.clone();
    let lc = log_cols(&h)?;
    let used: BTreeSet<usize> = [
        Some(lc.injury),
        lc.extracurricular,
        lc.removed,
        lc.returned_same_day,
        lc.written_notice,
        lc.ad_nurse,
        lc.form,
        lc.diagnosed,
        lc.diagnosis_date,
        lc.plan,
        lc.symptom_free,
        lc.full_academics,
        lc.clearance,
        lc.cleared_by,
        lc.full_return,
        lc.grade,
        lc.sport,
        lc.language,
    ]
    .into_iter()
    .flatten()
    .collect();
    let id_cols = crate::school::screenings::identifier_columns(&lt, &used);
    let year = SchoolYear::of(as_of);
    let mut detail = Vec::new();
    let mut gaps = [0u64; 7];
    let mut forms = 0u64;
    let mut students_ec: BTreeSet<String> = BTreeSet::new();
    let mut diagnosed_n = 0u64;
    let mut open = 0u64;
    let mut injuries = 0usize;
    let mut problems = Vec::new();
    let mut diagnosed_rows = Vec::new();
    let mut undated_notices = 0u64;
    for (ri, row) in lt.table.rows.iter().enumerate() {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let Some(inj) = crate::vax::dates::parse_date(cellv(row, Some(lc.injury))) else {
            problems.push((ri, "injury date missing or not a date".to_string()));
            continue;
        };
        if !year.contains(inj) {
            continue;
        }
        injuries += 1;
        let ec = lc.extracurricular.is_none_or(|i| yes(cellv(row, Some(i))));
        let nbd = add_business_days(inj, 1);
        let diagnosed = lc.diagnosed.is_some_and(|i| yes(cellv(row, Some(i))))
            || lc.diagnosis_date.is_some_and(|i| yes(cellv(row, Some(i))));
        let date_of = |i: Option<usize>| crate::vax::dates::parse_date(cellv(row, i));
        let mut results: Vec<String> = Vec::new();
        let mut check = |k: usize, ok: Option<bool>, results: &mut Vec<String>| {
            let s = match ok {
                Some(true) => "met",
                Some(false) => {
                    gaps[k] += 1;
                    "NOT MET"
                }
                None => "n/a",
            };
            results.push(s.to_string());
        };
        // 0 written notice; 1 AD and nurse. A "yes" without a date is done, but the
        // deadline cannot be checked, so it is not counted as a gap.
        for (k, column) in [(0usize, lc.written_notice), (1, lc.ad_nurse)] {
            if ec
                && date_of(column).is_none()
                && localdata::truthy(cellv(row, column)) == Some(true)
            {
                undated_notices += 1;
                results.push("done, date not recorded".to_string());
            } else {
                check(
                    k,
                    ec.then(|| date_of(column).is_some_and(|d| d <= nbd)),
                    &mut results,
                );
            }
        }
        // 2 form
        let form = lc.form.is_some_and(|i| yes(cellv(row, Some(i))));
        if form {
            forms += 1;
        }
        check(2, Some(form), &mut results);
        // 3 reentry plan
        check(
            3,
            diagnosed.then(|| lc.plan.is_some_and(|i| yes(cellv(row, Some(i))))),
            &mut results,
        );
        // 4 clearance before return
        let returned = date_of(lc.full_return);
        let clearance = date_of(lc.clearance);
        check(
            4,
            (ec && returned.is_some()).then(|| clearance.is_some_and(|c| Some(c) <= returned)),
            &mut results,
        );
        // 5 clearance after full academics
        let fa = date_of(lc.full_academics);
        check(
            5,
            clearance.map(|c| fa.is_some_and(|f| f <= c)),
            &mut results,
        );
        // 6 provider type
        let by = lc
            .cleared_by
            .map(|i| phn_core::text::normalize(cellv(row, Some(i))));
        check(
            6,
            by.filter(|b| !b.is_empty()).map(|b| {
                ALLOWED_CLEARERS.iter().any(|a| {
                    b == *a || b.split_whitespace().any(|w| w == *a) || b.contains(a) && a.len() > 3
                })
            }),
            &mut results,
        );
        if ec {
            let key = id_cols
                .iter()
                .map(|&i| cellv(row, Some(i)).to_string())
                .collect::<Vec<_>>()
                .join("|");
            students_ec.insert(if key.is_empty() {
                format!("row{ri}")
            } else {
                key
            });
        }
        if diagnosed {
            diagnosed_n += 1;
            diagnosed_rows.push(ri);
        }
        let is_open = diagnosed && returned.is_none() && fa.is_none();
        if is_open {
            open += 1;
        }
        let mut line = vec![(ri + 2).to_string()];
        line.extend(id_cols.iter().map(|&i| cellv(row, Some(i)).to_string()));
        line.push(inj.to_string());
        line.push(if ec { "yes".into() } else { "no".into() });
        line.push(if diagnosed { "yes".into() } else { "no".into() });
        line.extend(results);
        line.push(if is_open
            && inj
                .checked_add_signed(Duration::days(28))
                .is_some_and(|four| as_of >= four)
        {
            "over 4 weeks: assessment and 504/IEP review".into()
        } else if is_open {
            "open".into()
        } else {
            String::new()
        });
        if lc
            .returned_same_day
            .is_some_and(|i| localdata::truthy(cellv(row, Some(i))) == Some(true))
        {
            line.push("RETURNED SAME DAY (not allowed, 201.010(A))".into());
        } else {
            line.push(String::new());
        }
        detail.push(line);
    }
    // Workbook.
    let mut headers = vec!["Row".to_string()];
    headers.extend(id_cols.iter().map(|&i| h[i].clone()));
    headers.extend(["Injury date", "Extracurricular", "Diagnosed"].map(String::from));
    headers.extend(CHECKS.iter().map(|s| s.to_string()));
    headers.extend(["Status", "Alert"].map(String::from));
    let mut blocks = vec![
        Block::Paragraph(vec![Inline::plain(format!(
            "Head injury log checked against 105 CMR 201.000 for the {} school year on {as_of}. \
             Confidential student information.",
            year.label()
        ))]),
        Block::Table(TableBlock {
            caption: Some("Head injuries".into()),
            headers,
            rows: detail,
        }),
        Block::Table(TableBlock {
            caption: Some("Annual statistics for DPH (105 CMR 201.017)".into()),
            headers: vec!["Measure".into(), "Count".into()],
            rows: vec![
                vec!["Report of Head Injury forms received".into(), forms.to_string()],
                vec![
                    "Students with head injuries or suspected concussions in extracurricular athletics".into(),
                    students_ec.len().to_string(),
                ],
            ],
        }),
    ];
    if !problems.is_empty() {
        let mut hh = vec!["Row".to_string()];
        hh.extend(id_cols.iter().map(|&i| h[i].clone()));
        hh.push("Problem".into());
        blocks.push(Block::Table(TableBlock {
            caption: Some("Rows not checked".into()),
            headers: hh,
            rows: problems
                .iter()
                .map(|(ri, m)| {
                    let row = &lt.table.rows[*ri];
                    let mut x = vec![(ri + 2).to_string()];
                    x.extend(id_cols.iter().map(|&i| cellv(row, Some(i)).to_string()));
                    x.push(m.clone());
                    x
                })
                .collect(),
        }));
    }
    let doc = Document {
        title: "Head injury and concussion log".into(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    };
    let sandbox = ctx.sandbox()?;
    let out = sandbox.new_output_file(&format!("concussion_log_check_{as_of}.xlsx"))?;
    std::fs::write(&out, phn_docs::xlsx::to_xlsx(&doc)?)?;
    let output_file = sandbox.display_path(&out);

    // Suppressed counts.
    let threshold = ctx.suppression_threshold();
    let mut vals = vec![forms, students_ec.len() as u64, diagnosed_n, open];
    vals.extend(gaps.iter().copied());
    let matrix: Vec<Vec<Option<u64>>> = vals.iter().map(|v| vec![Some(*v)]).collect();
    let sup = suppress(
        &matrix,
        &SuppressionOptions {
            threshold,
            complementary: false,
            ..SuppressionOptions::default()
        },
    );
    let mut hide: Vec<bool> = (0..vals.len()).map(|i| sup.is_hidden(i, 0)).collect();
    // Forms received (0) plus records without a form (gap 2, at 4 + 2) is the number of
    // injuries, so when one of the two is hidden the other is hidden with it.
    if hide[0] != hide[6] {
        hide[0] = true;
        hide[6] = true;
    }
    let show = |i: usize| {
        if hide[i] {
            "*".to_string()
        } else {
            vals[i].to_string()
        }
    };
    let summary = LogSummary {
        file: lt.label.clone(),
        school_year: year.label(),
        injuries,
        head_injury_forms: show(0),
        students_extracurricular: show(1),
        diagnosed_concussions: show(2),
        open_cases: show(3),
        gaps: CHECKS
            .iter()
            .enumerate()
            .map(|(k, name)| (name.to_string(), show(4 + k)))
            .collect(),
        notices_without_date: (undated_notices > 0).then(|| {
            if undated_notices < u64::from(threshold) {
                "*".to_string()
            } else {
                undated_notices.to_string()
            }
        }),
        output_file: output_file.clone(),
        suppression_threshold: threshold,
    };

    // Plans and letters.
    let mut files = vec![output_file];
    let plans = if args.write_plans.unwrap_or(false) && !diagnosed_rows.is_empty() {
        if lc.student_name.is_none() && lc.first.is_none() && lc.last.is_none() {
            return Err(PhnError::InvalidInput(
                "plans need a 'Student Name' column (or 'First Name' and 'Last Name') in the log"
                    .into(),
            ));
        }
        let format = args.format.unwrap_or_default();
        let (dir, shown) = templates::output_folder(ctx, &format!("concussion_plans_{as_of}"))?;
        let plan_md = reentry_plan_template();
        let mut resolver = TemplateResolver::new(
            ctx,
            "concussion_caregiver_letter",
            CAREGIVER_LETTER.to_string(),
        );
        let mut plan_docs = Vec::new();
        let mut letters: BTreeMap<String, (String, bool, Vec<Document>)> = BTreeMap::new();
        let mut fallback = 0usize;
        let mut unknown_placeholders: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        for ri in &diagnosed_rows {
            let row = &lt.table.rows[*ri];
            let name = match lc.student_name {
                Some(i) => cellv(row, Some(i)).to_string(),
                None => format!("{} {}", cellv(row, lc.first), cellv(row, lc.last))
                    .trim()
                    .to_string(),
            };
            let mut v = BTreeMap::new();
            v.insert("student_name".to_string(), name);
            v.insert(
                "parent_name".to_string(),
                lc.parent
                    .map(|i| cellv(row, Some(i)).to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "Parent or Guardian".into()),
            );
            v.insert("grade".to_string(), cellv(row, lc.grade).to_string());
            v.insert(
                "school".to_string(),
                args.school_name.clone().unwrap_or_default(),
            );
            v.insert("sport".to_string(), cellv(row, lc.sport).to_string());
            v.insert(
                "injury_date".to_string(),
                cellv(row, Some(lc.injury)).to_string(),
            );
            v.insert(
                "diagnosis_date".to_string(),
                lc.diagnosis_date
                    .map(|i| cellv(row, Some(i)).to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| cellv(row, Some(lc.injury)).to_string()),
            );
            v.insert(
                "nurse_name".to_string(),
                args.nurse_name.clone().unwrap_or_default(),
            );
            v.insert(
                "nurse_phone".to_string(),
                args.nurse_phone.clone().unwrap_or_default(),
            );
            v.insert("today".to_string(), as_of.to_string());
            let (filled, _) = templates::fill(&plan_md, &v, "________");
            plan_docs.push(templates::document(
                ctx,
                &filled,
                "en-US",
                "Graduated reentry plan",
            ));
            let lang = cellv(row, lc.language).to_string();
            let chosen = resolver.choose(&lang);
            if !chosen.translated && !templates::is_english(&lang) {
                fallback += 1;
            }
            let (filled, unk) = templates::fill(&chosen.markdown, &v, "________");
            unknown_placeholders.extend(unk);
            let d = templates::document(ctx, &filled, &chosen.tag, "Concussion letter");
            // Unrecognized language values are grouped as "Other" (no cell text in results
            // or file names).
            letters
                .entry(result_language_label(&lang))
                .or_insert_with(|| (chosen.tag.clone(), chosen.translated, Vec::new()))
                .2
                .push(d);
        }
        let n_plans = plan_docs.len();
        let combined = templates::combine("Graduated reentry plans", "en-US", plan_docs, ctx);
        files.push(templates::write_in(
            ctx,
            &dir,
            &format!("reentry_plans.{}", format.ext()),
            &format.render(&combined)?,
        )?);
        let mut by_language = Vec::new();
        let mut n_letters = 0;
        for (lang, (tag, translated, docs)) in letters {
            n_letters += docs.len();
            by_language.push(LanguageCount {
                language: lang.clone(),
                documents: docs.len(),
                agency_translation: translated,
            });
            let combined = templates::combine("Concussion letters", &tag, docs, ctx);
            files.push(templates::write_in(
                ctx,
                &dir,
                &format!(
                    "caregiver_letters_{}.{}",
                    lang.to_lowercase().replace(' ', "_"),
                    format.ext()
                ),
                &format.render(&combined)?,
            )?);
        }
        files.push(templates::write_in(
            ctx,
            &dir,
            "template_english_concussion_caregiver_letter.md",
            CAREGIVER_LETTER.as_bytes(),
        )?);
        Some(PlansWritten {
            folder: shown,
            plans: n_plans,
            letters: n_letters,
            files: files[1..].to_vec(),
            by_language,
            english_fallback: fallback,
            unknown_placeholders: unknown_placeholders.into_iter().collect(),
        })
    } else {
        None
    };
    let label = lt.label.clone();
    Ok((summary, plans, files, label, lt.identifier_categories()))
}

// ------------------------------------------------------------------ tool

/// Concussion requirements, plans, and tracking.
pub fn concussion_return_to_learn_tracker(
    ctx: &Context,
    args: ConcussionArgs,
) -> Result<Envelope<ConcussionResult>> {
    let c = &*CONCUSSION_INFO;
    let src = &c.sources;
    let as_of = match &args.as_of {
        Some(s) => crate::vax::dates::require_date("as_of", s)?,
        None => chrono::Local::now().date_naive(),
    };
    let guidance = args.rest_guidance.unwrap_or_default();
    let (timeline, mut flags, return_to_play, monitoring_schedule) = single(&args, as_of)?;
    let symptoms = args.symptoms.clone().unwrap_or_default();
    let accommodations = accommodations_for(&symptoms);
    let symptom_score = match &args.symptom_ratings {
        Some(r) if !r.is_empty() => Some(score(r)?),
        _ => None,
    };
    let return_to_learn: Vec<StageOut> = c
        .return_to_learn
        .iter()
        .map(|s| stage_out(s, None))
        .collect();
    let forms: Vec<FormOut> = c
        .forms
        .iter()
        .map(|f| FormOut {
            name: f.name.clone(),
            who_completes: f.who_completes.trim().to_string(),
            when: f.when.clone(),
            url: f.url.clone(),
            translations: f.translations.clone(),
            citation: cite(&f.source, &f.section),
        })
        .collect();
    let (log, plans, files_written, local_label, identifiers) = match args.tracker_path.as_deref() {
        Some(p) if !p.trim().is_empty() => {
            let (l, p, f, label, ids) = log_mode(ctx, &args, as_of)?;
            (Some(l), p, f, Some(label), ids)
        }
        _ => {
            if args.write_plans.unwrap_or(false) {
                flags.push(
                    "write_plans needs tracker_path: point the tool at your head injury log in /data so plans are filled locally. The plan template and placeholders are returned instead.".into(),
                );
            }
            (None, None, Vec::new(), None, Vec::new())
        }
    };

    // ---- Markdown
    let mut s =
        format!("**Concussion: requirements and return-to-learn plan** (evaluated {as_of})\n\n");
    if let Some(l) = &log {
        s.push_str(&format!(
            "**Head injury log ({} school year):** {} injury record(s). Report of Head Injury forms: {}; students with head injuries or suspected concussions in extracurricular athletics: {} (105 CMR 201.017); diagnosed concussions: {}; open cases: {}. Per-student checks are in `{}` (not shown here).\n\n",
            l.school_year, l.injuries, l.head_injury_forms, l.students_extracurricular,
            l.diagnosed_concussions, l.open_cases, l.output_file
        ));
        let rows: Vec<Vec<String>> = l
            .gaps
            .iter()
            .map(|(k, v)| vec![k.clone(), v.clone()])
            .collect();
        s.push_str(&md::table(
            &["Requirement", "Records not meeting it"],
            &rows,
        ));
        if let Some(n) = &l.notices_without_date {
            s.push_str(&format!(
                "\nNotices marked done without a date (the next-business-day deadline could not be checked; not counted above): {n}.\n"
            ));
        }
        s.push('\n');
    }
    if !timeline.is_empty() {
        let rows: Vec<Vec<String>> = timeline
            .iter()
            .map(|t| {
                vec![
                    t.date.clone().unwrap_or_else(|| "—".into()),
                    t.what.clone(),
                    t.status.clone(),
                    t.citation.clone(),
                ]
            })
            .collect();
        s.push_str(&md::table(&["Date", "Step", "Status", "Source"], &rows));
    }
    for f in &flags {
        s.push_str(&format!("\n**Check:** {f}\n"));
    }
    s.push_str("\n**Return to learn (DPH 2018)**\n\n");
    for st in &return_to_learn {
        s.push_str(&format!(
            "- **{}** ({}): {}\n",
            st.name,
            st.typical_days.clone().unwrap_or_default(),
            st.text
        ));
    }
    if guidance == RestGuidance::Cdc2025 {
        s.push_str("\nYour district follows CDC (2025): most children can return to school within 1 to 2 days, with supports; getting back to school can shorten recovery.\n");
    }
    s.push_str("\n**Gradual return to play (DPH clearance form)**\n\n");
    for st in &return_to_play {
        s.push_str(&format!(
            "- {} — {}{}\n",
            st.name,
            st.text,
            st.status
                .as_deref()
                .map(|x| format!(" _[{x}]_"))
                .unwrap_or_default()
        ));
    }
    if let Some(sc) = &symptom_score {
        s.push_str(&format!(
            "\n**Symptom score (DPH monitoring tool):** {} of {} with {} symptom(s) present.{}\n",
            sc.total,
            sc.max_possible,
            sc.symptoms_present,
            if sc.unrecognized.is_empty() {
                String::new()
            } else {
                format!(
                    " Not on DPH's list (ignored): {}.",
                    sc.unrecognized.join(", ")
                )
            }
        ));
    }
    if !monitoring_schedule.is_empty() {
        s.push_str(&format!(
            "\n**Symptom checks** (twice weekly for 2 weeks, then weekly; tool's suggested days): {}\n",
            monitoring_schedule.join(", ")
        ));
    }
    if !symptoms.is_empty() {
        s.push_str("\n**Accommodations for the symptoms given (DPH Table 1)**\n\n");
        for a in &accommodations {
            s.push_str(&format!("- **{}**: {}\n", a.effect, a.supports.join("; ")));
        }
    }
    s.push_str("\n**Danger signs: call 911**\n\n");
    let danger = outs(&c.danger_signs, src);
    for d in &danger {
        s.push_str(&format!("- {} ({})\n", d.text, d.citation));
    }
    s.push_str("\n**Forms**\n\n");
    for f in &forms {
        s.push_str(&format!(
            "- {} — {}; {} ({})\n",
            f.name, f.who_completes, f.when, f.url
        ));
    }
    let divergences = outs(&c.divergences, src);
    s.push_str("\n**Where the guidance differs**\n\n");
    for d in &divergences {
        s.push_str(&format!(
            "- {}{}\n",
            d.text,
            confidence_suffix(d.confidence)
        ));
    }
    if let Some(p) = &plans {
        s.push_str(&format!(
            "\n**Documents:** {} reentry plan(s) and {} caregiver letter(s) written to `{}`.{}\n",
            p.plans,
            p.letters,
            p.folder,
            if p.english_fallback > 0 {
                format!(
                    " {} letter(s) are in English because the agency has no translation for the family's language. {}",
                    p.english_fallback,
                    templates::translation_help(&["concussion_caregiver_letter"])
                )
            } else {
                String::new()
            }
        ));
    }

    if let Some(p) = &plans
        && !p.unknown_placeholders.is_empty()
    {
        s.push_str(&format!(
            "Your agency template uses placeholder(s) the tool does not fill (left as [name]): {}.\n",
            p.unknown_placeholders.join(", ")
        ));
    }
    let mut caveats = content::review(&[content::CONCUSSION]);
    caveats.push(
        "105 CMR 201.000 covers extracurricular athletic activities in public middle and high \
         schools and MIAA schools; districts may apply their policies to all head injuries."
            .into(),
    );
    caveats.push(
        "Business-day deadlines skip weekends but not holidays or school vacations; adjust for your \
         calendar."
            .into(),
    );
    caveats.push(
        "Return-to-play decisions are medical decisions by an authorized provider (105 CMR \
         201.011); this tool tracks the steps and does not clear anyone."
            .into(),
    );
    let mut sources = Vec::new();
    if let Some(l) = local_label {
        sources.push(Source::local_file(l));
    }
    sources.extend(content::sources_for(
        src,
        [
            "cmr201",
            "dph-rtl-2018",
            "dph-clearance-form",
            "cdc-return-school",
            "cdc-return-sports",
            "cdc-signs",
            "dph-symptom-tool",
        ],
    ));
    let mut placeholders: Vec<String> = PLAN_PLACEHOLDERS.iter().map(|x| x.to_string()).collect();
    placeholders.sort();
    let privacy = if log.is_some() {
        Privacy {
            row_level_data: false,
            suppression_applied: true,
            suppression_threshold: Some(ctx.suppression_threshold()),
            identifiers_detected: identifiers,
            files_written,
            notes: vec!["Only counts are returned; per-student checks, plans, and letters are in the output folder.".into()],
        }
    } else {
        Privacy {
            notes: vec!["Only dates were used; no student name or identifier was taken.".into()],
            ..Privacy::none()
        }
    };
    Ok(Envelope::new(
        s,
        ConcussionResult {
            evaluated_on: as_of.to_string(),
            rest_guidance: guidance,
            timeline,
            flags,
            return_to_learn,
            return_to_play,
            accommodations,
            symptom_score,
            monitoring_schedule,
            forms,
            danger_signs: danger,
            reentry_plan_requirements: outs(&c.reentry_plan, src),
            clearance_rules: outs(&c.clearance, src),
            divergences,
            template_placeholders: placeholders,
            log,
            plans,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy)
    .decision_support())
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
    fn deadlines_and_gates_for_one_student() {
        let (_d, c) = ctx();
        // Friday injury: written notice due Monday.
        let e = concussion_return_to_learn_tracker(
            &c,
            ConcussionArgs {
                injury_date: Some("2026-10-02".into()),
                diagnosis_date: Some("2026-10-03".into()),
                returned_to_school: Some("2026-10-07".into()),
                cleared_to_begin_rtp: Some("2026-10-12".into()),
                full_academics_date: Some("2026-10-14".into()),
                rtp_stage_completed: Some(2),
                symptoms: Some(vec!["headache".into(), "bothered by light".into()]),
                symptom_ratings: Some(BTreeMap::from([
                    ("Headache".to_string(), 3),
                    ("Sensitivity to light".to_string(), 2),
                    ("Zzz".to_string(), 1),
                ])),
                as_of: Some("2026-11-05".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &e.data;
        assert!(d.timeline.iter().any(
            |t| t.date.as_deref() == Some("2026-10-05") && t.what.starts_with("Written notice")
        ));
        assert!(
            d.flags
                .iter()
                .any(|f| f.contains("before the student was back in class"))
        );
        assert!(d.flags.iter().any(|f| f.contains("symptom-free date")));
        assert_eq!(d.return_to_play[1].status.as_deref(), Some("completed"));
        assert_eq!(d.return_to_play[2].status.as_deref(), Some("next"));
        let sc = d.symptom_score.as_ref().unwrap();
        assert_eq!(
            (sc.total, sc.symptoms_present, sc.max_possible),
            (5, 2, 132)
        );
        assert_eq!(sc.unrecognized, vec!["Zzz".to_string()]);
        assert!(d.accommodations.iter().any(|a| a.effect == "Headaches"));
        assert!(
            d.accommodations
                .iter()
                .any(|a| a.effect.starts_with("Light"))
        );
        assert_eq!(
            d.monitoring_schedule.first().map(String::as_str),
            Some("2026-10-07")
        );
        assert!(e.notice.is_some());
    }

    #[test]
    fn four_week_flag_and_bad_dates() {
        let (_d, c) = ctx();
        let e = concussion_return_to_learn_tracker(
            &c,
            ConcussionArgs {
                injury_date: Some("2026-09-15".into()),
                diagnosis_date: Some("2026-09-16".into()),
                as_of: Some("2026-10-20".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(e.data.flags.iter().any(|f| f.contains("4 weeks")));
        assert!(
            concussion_return_to_learn_tracker(
                &c,
                ConcussionArgs {
                    injury_date: Some("2026-09-15".into()),
                    diagnosis_date: Some("2026-09-10".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn log_mode_counts_and_plans() {
        let (d, c) = ctx();
        std::fs::write(
            d.path().join("data/log.csv"),
            "Student Name,Student ID,Grade,Sport,Home Language,Injury Date,Extracurricular,Written Notice,AD Nurse Notified,Head Injury Form,Diagnosed Concussion,Reentry Plan,Full Academics,Clearance Date,Cleared By,Full Return\n\
Quill Zorvath,9200001,10,Soccer,Spanish,2026-09-18,Y,2026-09-21,2026-09-21,Y,Y,2026-09-22,2026-10-01,2026-10-03,Physician,2026-10-09\n\
Oro Brembly,9200002,11,Football,English,2026-09-25,Y,2026-09-30,2026-09-26,Y,Y,,,2026-10-02,Coach,2026-10-05\n\
Iska Quenby,9200003,9,Hockey,English,2026-10-10,Y,,,N,N,,,,,\n",
        )
        .unwrap();
        let e = concussion_return_to_learn_tracker(
            &c,
            ConcussionArgs {
                tracker_path: Some("log.csv".into()),
                write_plans: Some(true),
                as_of: Some("2026-11-01".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let l = e.data.log.as_ref().unwrap();
        assert_eq!(l.injuries, 3);
        // Counts 1-4 are suppressed.
        assert_eq!(l.head_injury_forms, "*");
        let p = e.data.plans.as_ref().unwrap();
        assert_eq!((p.plans, p.letters, p.english_fallback), (2, 2, 1));
        let all = serde_json::to_string(&e).unwrap();
        for v in ["Zorvath", "Quill", "9200001", "Brembly"] {
            assert!(!all.contains(v), "{v} leaked");
        }
        assert!(d.path().join("data").join(&l.output_file).exists());
    }
}
