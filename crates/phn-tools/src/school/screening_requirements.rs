//! SCH-01 `get_ma_school_screening_requirements`.
//!
//! Lists the school health screenings required in Massachusetts (vision,
//! hearing, height/weight and BMI, postural, SBIRT, the kindergarten vision
//! certificate, and physical examinations) by grade, with protocols, referral
//! criteria, parent notification, exemptions, reporting, and confidentiality
//! rules, each cited to 105 CMR 200.000, M.G.L. c. 71, or DPH protocols.

use phn_core::{Envelope, PhnError, Privacy, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;
use crate::school::calendar::Grade;
use crate::school::content::{
    self, CitedOut, Confidence, Frequency, SCREENINGS, ScheduleBand, Screening, confidence_suffix,
    outs,
};

/// Arguments for `get_ma_school_screening_requirements`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ScreeningRequirementsArgs {
    /// Screening to show: vision, hearing, BMI (height and weight), postural
    /// (scoliosis), SBIRT, kindergarten vision certificate, or physical exam.
    /// Default: all.
    pub screening: Option<String>,
    /// Grade: PK, K, 1-12, or "ungraded" (then give age_years). Default: the
    /// requirements for every grade as a table.
    pub grade: Option<String>,
    /// Age in years, for ungraded classrooms.
    pub age_years: Option<u32>,
    /// The student is new to the district this year (school-entry rules apply).
    pub new_student: Option<bool>,
    /// Include protocol, referral, notification, and reporting details
    /// (default true; false gives the schedule only).
    pub include_details: Option<bool>,
}

/// One band of a screening schedule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BandOut {
    pub band: String,
    pub grades: Vec<String>,
    pub frequency: Frequency,
    pub text: String,
    pub citation: String,
}

/// Whether a screening is required for the requested grade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GradeRequirement {
    pub grade: String,
    /// "required", "required at school entry", "district-selected grade", or
    /// "not required".
    pub status: String,
    pub detail: String,
    pub citation: Option<String>,
}

/// One screening with its rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScreeningOut {
    pub id: String,
    pub name: String,
    pub legal_basis: Vec<String>,
    pub schedule: Vec<BandOut>,
    /// Present when a grade was given.
    pub for_grade: Option<GradeRequirement>,
    pub new_students: Option<CitedOut>,
    pub who: Vec<CitedOut>,
    pub protocol: Vec<CitedOut>,
    pub referral: Vec<CitedOut>,
    pub parent_notification: Vec<CitedOut>,
    pub exemptions: Vec<CitedOut>,
    pub reporting: Vec<CitedOut>,
    pub confidentiality: Vec<CitedOut>,
    /// Whether track_school_screenings can track it from a roster.
    pub trackable: bool,
    pub confidence: Confidence,
}

/// One row of the grade-by-screening table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GradeRow {
    pub grade: String,
    /// Screening id -> requirement ("annual", "once in band", "-", ...).
    pub requirements: Vec<(String, String)>,
}

/// Result of `get_ma_school_screening_requirements`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScreeningRequirementsResult {
    pub grade: Option<String>,
    pub screenings: Vec<ScreeningOut>,
    /// Requirements for every grade (when no grade was given).
    pub grade_table: Vec<GradeRow>,
    /// Rules that apply to every screening (exemptions, waivers, private schools).
    pub general: Vec<CitedOut>,
}

/// Find screenings matching a query (id, name, or alias).
pub fn find_screening(q: &str) -> Vec<&'static Screening> {
    let n = phn_core::text::normalize(q);
    let all = &SCREENINGS.screenings;
    let exact: Vec<&Screening> = all
        .iter()
        .filter(|s| {
            phn_core::text::normalize(&s.id.replace('_', " ")) == n
                || phn_core::text::normalize(&s.name) == n
                || s.aliases.iter().any(|a| phn_core::text::normalize(a) == n)
        })
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    let mut scored: Vec<(&Screening, f64)> = all
        .iter()
        .map(|s| {
            let mut names = vec![s.id.replace('_', " "), s.name.clone()];
            names.extend(s.aliases.iter().cloned());
            let best = names
                .iter()
                .map(|x| {
                    let xn = phn_core::text::normalize(x);
                    if xn.len() >= 3 && (n.contains(&xn) || xn.contains(&n)) {
                        0.9
                    } else {
                        phn_core::text::similarity(&n, &xn)
                    }
                })
                .fold(0.0, f64::max);
            (s, best)
        })
        .filter(|(_, sc)| *sc >= 0.6)
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.into_iter().map(|(s, _)| s).collect()
}

/// The schedule band that applies to a student, if any.
///
/// Ungraded students are matched by age to bands with ages; a new student
/// matches school-entry bands in any grade where the screening is done at
/// entry (vision and hearing are screened "in the year of school entry").
pub fn band_for(sc: &Screening, grade: Grade, age: Option<i32>) -> Option<&ScheduleBand> {
    match grade {
        Grade::Ungraded => {
            let a = age?;
            sc.schedule
                .iter()
                .find(|b| {
                    b.ages
                        .is_some_and(|[lo, hi]| a >= lo as i32 && a <= hi as i32)
                })
                .or_else(|| {
                    sc.schedule
                        .iter()
                        .find(|b| b.frequency == Frequency::EntryAndInterval)
                })
        }
        g => {
            let label = g.label();
            sc.schedule
                .iter()
                .find(|b| b.grades.iter().any(|x| x.eq_ignore_ascii_case(&label)))
        }
    }
}

fn requirement_for(
    sc: &Screening,
    grade: Grade,
    age: Option<i32>,
    new_student: bool,
) -> GradeRequirement {
    let label = match (grade, age) {
        (Grade::Ungraded, Some(a)) => format!("ungraded, age {a}"),
        (g, _) => g.label(),
    };
    let src = &SCREENINGS.sources;
    if grade == Grade::Ungraded && age.is_none() {
        return GradeRequirement {
            grade: label,
            status: "needs age".into(),
            detail: "For ungraded classrooms the rules go by age; give age_years.".into(),
            citation: None,
        };
    }
    match band_for(sc, grade, age) {
        Some(b) => {
            let status = match b.frequency {
                Frequency::Annual => "required (every year in this band)",
                Frequency::OnceInBand => "required once in this band",
                Frequency::SchoolEntry => "required at school entry",
                Frequency::DistrictSelected => "required if the district selected this grade",
                Frequency::EntryAndInterval => "required at entry, then every 3 or 4 years",
                Frequency::Event => "required when the event occurs",
            };
            GradeRequirement {
                grade: label,
                status: status.into(),
                detail: b.text.trim().to_string(),
                citation: Some(content::citation(src, &b.source, &b.section)),
            }
        }
        None => {
            if grade == Grade::Ungraded && sc.schedule.iter().all(|b| b.ages.is_none()) {
                // Rules stated as birthdays (BMI) or grades only (postural).
                let ungraded: Vec<&ScheduleBand> = sc
                    .schedule
                    .iter()
                    .filter(|b| b.text.contains("ungraded"))
                    .collect();
                return match ungraded.first() {
                    Some(b) => GradeRequirement {
                        grade: label,
                        status: "check the ungraded rule".into(),
                        detail: b.text.trim().to_string(),
                        citation: Some(content::citation(src, &b.source, &b.section)),
                    },
                    None => GradeRequirement {
                        grade: label,
                        status: "grades only".into(),
                        detail: "The rule is stated by grade only; apply it by the student's age-equivalent grade under your district's policy.".into(),
                        citation: sc
                            .schedule
                            .first()
                            .map(|b| content::citation(src, &b.source, &b.section)),
                    },
                };
            }
            if new_student && let Some(n) = &sc.new_students {
                return GradeRequirement {
                    grade: label,
                    status: "new students: screen at entry".into(),
                    detail: n.text.trim().to_string(),
                    citation: Some(content::citation(src, &n.source, &n.section)),
                };
            }
            let event: Vec<&ScheduleBand> = sc
                .schedule
                .iter()
                .filter(|b| b.frequency == Frequency::Event)
                .collect();
            GradeRequirement {
                grade: label,
                status: if event.is_empty() {
                    "not required in this grade".into()
                } else {
                    "required only when the event occurs".into()
                },
                detail: if event.is_empty() {
                    "Not a required screening grade (the district may screen more often under 105 CMR 200.600).".into()
                } else {
                    event
                        .iter()
                        .map(|b| b.text.trim().to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                },
                citation: None,
            }
        }
    }
}

fn short(sc: &Screening, grade: Grade) -> String {
    match band_for(sc, grade, None) {
        Some(b) => match b.frequency {
            Frequency::Annual => "yes".into(),
            Frequency::OnceInBand => format!("once in {}", b.band),
            Frequency::SchoolEntry => "at entry".into(),
            Frequency::DistrictSelected => "if selected".into(),
            Frequency::EntryAndInterval => "entry, then 3-4 yr".into(),
            Frequency::Event => "event".into(),
        },
        None => "-".into(),
    }
}

/// Requirements for school health screenings by grade.
pub fn get_ma_school_screening_requirements(
    _ctx: &Context,
    args: ScreeningRequirementsArgs,
) -> Result<Envelope<ScreeningRequirementsResult>> {
    let c = &*SCREENINGS;
    let src = &c.sources;
    let details = args.include_details.unwrap_or(true);
    let selected: Vec<&Screening> = match args.screening.as_deref().map(str::trim) {
        Some(q) if !q.is_empty() => {
            let found = find_screening(q);
            if found.is_empty() {
                return Err(PhnError::NotFound(format!(
                    "no screening matches '{q}'. Available: {}",
                    c.screenings
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            found.into_iter().take(1).collect()
        }
        _ => c.screenings.iter().collect(),
    };
    let grade = match args.grade.as_deref().map(str::trim) {
        Some(g) if !g.is_empty() => Some(Grade::parse(g)?),
        _ => None,
    };
    let age = args.age_years.map(|a| a as i32);
    if grade == Some(Grade::Ungraded) && age.is_none() {
        return Err(PhnError::InvalidInput(
            "for an ungraded classroom give age_years; the rules for ungraded classrooms go by age"
                .into(),
        ));
    }
    let new_student = args.new_student.unwrap_or(false);

    let screenings: Vec<ScreeningOut> = selected
        .iter()
        .map(|sc| ScreeningOut {
            id: sc.id.clone(),
            name: sc.name.clone(),
            legal_basis: sc
                .legal_basis
                .iter()
                .map(|l| content::citation(src, &l.source, &l.section))
                .collect(),
            schedule: sc
                .schedule
                .iter()
                .map(|b| BandOut {
                    band: b.band.clone(),
                    grades: b.grades.clone(),
                    frequency: b.frequency,
                    text: b.text.trim().to_string(),
                    citation: content::citation(src, &b.source, &b.section),
                })
                .collect(),
            for_grade: grade.map(|g| requirement_for(sc, g, age, new_student)),
            new_students: sc.new_students.as_ref().map(|n| n.out(src)),
            who: if details { outs(&sc.who, src) } else { vec![] },
            protocol: if details {
                outs(&sc.protocol, src)
            } else {
                vec![]
            },
            referral: if details {
                outs(&sc.referral, src)
            } else {
                vec![]
            },
            parent_notification: if details {
                outs(&sc.parent_notification, src)
            } else {
                vec![]
            },
            exemptions: if details {
                outs(&sc.exemptions, src)
            } else {
                vec![]
            },
            reporting: if details {
                outs(&sc.reporting, src)
            } else {
                vec![]
            },
            confidentiality: if details {
                outs(&sc.confidentiality, src)
            } else {
                vec![]
            },
            trackable: sc.trackable,
            confidence: sc.confidence,
        })
        .collect();

    let grade_table: Vec<GradeRow> = if grade.is_none() {
        Grade::all()
            .into_iter()
            .map(|g| GradeRow {
                grade: g.label(),
                requirements: selected
                    .iter()
                    .filter(|s| s.id != "physical_exam")
                    .map(|s| (s.id.clone(), short(s, g)))
                    .collect(),
            })
            .collect()
    } else {
        Vec::new()
    };

    // ---- Markdown
    let mut s = String::from("**Massachusetts school health screening requirements**");
    if let Some(g) = grade {
        s.push_str(&format!(
            " — {}",
            match (g, age) {
                (Grade::Ungraded, Some(a)) => format!("ungraded classroom, age {a}"),
                (g, _) => format!("grade {}", g.label()),
            }
        ));
    }
    s.push_str("\n\n");
    if grade.is_some() {
        let rows: Vec<Vec<String>> = screenings
            .iter()
            .filter_map(|o| {
                o.for_grade.as_ref().map(|r| {
                    vec![
                        o.name.clone(),
                        r.status.clone(),
                        r.detail.clone(),
                        r.citation.clone().unwrap_or_else(|| "—".into()),
                    ]
                })
            })
            .collect();
        s.push_str(&md::table(
            &["Screening", "This grade", "Rule", "Source"],
            &rows,
        ));
    } else {
        let names: Vec<String> = selected
            .iter()
            .filter(|x| x.id != "physical_exam")
            .map(|x| x.name.replace(" screening", ""))
            .collect();
        let mut headers = vec!["Grade"];
        headers.extend(names.iter().map(String::as_str));
        let rows: Vec<Vec<String>> = grade_table
            .iter()
            .map(|r| {
                let mut v = vec![r.grade.clone()];
                v.extend(r.requirements.iter().map(|(_, x)| x.clone()));
                v
            })
            .collect();
        if !rows.is_empty() && headers.len() > 1 {
            s.push_str(&md::table(&headers, &rows));
        }
    }
    for o in &screenings {
        s.push_str(&format!("\n**{}**\n\n", o.name));
        for b in &o.schedule {
            s.push_str(&format!("- {} ({})\n", b.text, b.citation));
        }
        if let Some(n) = &o.new_students {
            s.push_str(&format!(
                "- New students: {}{}\n",
                n.text,
                confidence_suffix(n.confidence)
            ));
        }
        if details {
            for (label, list) in [
                ("Who screens", &o.who),
                ("Protocol", &o.protocol),
                ("Referral and follow-up", &o.referral),
                ("Parent notification", &o.parent_notification),
                ("Exemptions and opt-out", &o.exemptions),
                ("Reporting to DPH", &o.reporting),
                ("Confidentiality", &o.confidentiality),
            ] {
                if list.is_empty() {
                    continue;
                }
                s.push_str(&format!("\n_{label}_\n"));
                for x in list {
                    s.push_str(&format!(
                        "- {} ({}){}\n",
                        x.text,
                        x.citation,
                        confidence_suffix(x.confidence)
                    ));
                }
            }
        }
    }
    let general = outs(&c.general, src);
    if details && selected.len() > 1 {
        s.push_str("\n**Rules for every screening**\n\n");
        for g in &general {
            s.push_str(&format!("- {} ({})\n", g.text, g.citation));
        }
    }

    let mut keys: Vec<&str> = vec!["cmr200"];
    for sc in &selected {
        keys.extend(sc.legal_basis.iter().map(|l| l.source.as_str()));
        keys.extend(sc.schedule.iter().map(|b| b.source.as_str()));
        if details {
            for list in [
                &sc.who,
                &sc.protocol,
                &sc.referral,
                &sc.parent_notification,
                &sc.exemptions,
                &sc.reporting,
                &sc.confidentiality,
            ] {
                keys.extend(list.iter().map(|x| x.source.as_str()));
            }
        }
    }
    let mut caveats = content::review(&[content::SCREENING]);
    if selected.iter().any(|s| s.id == "hearing") {
        caveats.push(
            "Hearing protocol details come from DPH's 2007 manual because DPH's current Pure Tone \
             Audiometer protocol could not be retrieved; confirm pass and rescreen criteria against \
             the current DPH protocol."
                .into(),
        );
    }
    caveats.push(
        "These are state minimums; your school committee or board of health may require more \
         (105 CMR 200.600). Private schools follow different rules (M.G.L. c. 71, section 57)."
            .into(),
    );
    if selected.iter().any(|s| s.id == "sbirt") {
        caveats.push(
            "SBIRT results may never be recorded in a form that identifies the student (M.G.L. c. 71, \
             section 97(c)); track SBIRT only as de-identified counts."
                .into(),
        );
    }
    Ok(Envelope::new(
        s,
        ScreeningRequirementsResult {
            grade: grade.map(|g| match (g, age) {
                (Grade::Ungraded, Some(a)) => format!("ungraded, age {a}"),
                (g, _) => g.label(),
            }),
            screenings,
            grade_table,
            general,
        },
    )
    .sources(content::sources_for(src, keys))
    .caveats(caveats)
    .privacy(Privacy::none())
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    fn sc(id: &str) -> &'static Screening {
        SCREENINGS.get(id).unwrap()
    }

    #[test]
    fn grade_rules_match_105_cmr_200() {
        // Vision: annual K-5, once 6-8 and 9-12; PK annual per DPH protocol.
        for (g, f) in [
            (Grade::PK, Frequency::Annual),
            (Grade::K, Frequency::Annual),
            (Grade::G(5), Frequency::Annual),
            (Grade::G(7), Frequency::OnceInBand),
            (Grade::G(11), Frequency::OnceInBand),
        ] {
            assert_eq!(
                band_for(sc("vision"), g, None).unwrap().frequency,
                f,
                "{g:?}"
            );
        }
        // Hearing: annual K-3, none in 4-5, once 6-8 and 9-12.
        assert_eq!(
            band_for(sc("hearing"), Grade::G(3), None)
                .unwrap()
                .frequency,
            Frequency::Annual
        );
        assert!(band_for(sc("hearing"), Grade::G(4), None).is_none());
        assert!(band_for(sc("hearing"), Grade::G(5), None).is_none());
        assert!(band_for(sc("hearing"), Grade::PK, None).is_none());
        // BMI grades 1, 4, 7, 10 only.
        let bmi: Vec<String> = Grade::all()
            .into_iter()
            .filter(|g| band_for(sc("height_weight_bmi"), *g, None).is_some())
            .map(Grade::label)
            .collect();
        assert_eq!(bmi, vec!["1", "4", "7", "10"]);
        // Postural annually in grades 5-9.
        let post: Vec<String> = Grade::all()
            .into_iter()
            .filter(|g| band_for(sc("postural"), *g, None).is_some())
            .map(Grade::label)
            .collect();
        assert_eq!(post, vec!["5", "6", "7", "8", "9"]);
        // Ungraded by age: vision annually through 11, hearing through 9.
        assert!(band_for(sc("vision"), Grade::Ungraded, Some(11)).is_some());
        assert!(band_for(sc("hearing"), Grade::Ungraded, Some(10)).is_none());
        let bmi_ungraded =
            requirement_for(sc("height_weight_bmi"), Grade::Ungraded, Some(10), false);
        assert_eq!(bmi_ungraded.status, "check the ungraded rule");
        assert_eq!(
            requirement_for(sc("hearing"), Grade::Ungraded, Some(10), false).status,
            "not required in this grade"
        );
        assert_eq!(
            band_for(sc("hearing"), Grade::Ungraded, Some(13))
                .unwrap()
                .frequency,
            Frequency::OnceInBand
        );
    }

    #[test]
    fn one_grade_answer_is_cited() {
        let (_d, c) = ctx();
        let e = get_ma_school_screening_requirements(
            &c,
            ScreeningRequirementsArgs {
                grade: Some("4".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let v = e
            .data
            .screenings
            .iter()
            .find(|s| s.id == "height_weight_bmi")
            .unwrap();
        assert!(v.for_grade.as_ref().unwrap().status.starts_with("required"));
        let h = e
            .data
            .screenings
            .iter()
            .find(|s| s.id == "hearing")
            .unwrap();
        assert_eq!(
            h.for_grade.as_ref().unwrap().status,
            "not required in this grade"
        );
        assert!(e.summary.contains("200.500"));
        assert!(e.notice.is_some());
        assert!(
            e.sources
                .iter()
                .any(|s| s.title.contains("105 CMR 200.000"))
        );
    }

    #[test]
    fn table_for_all_grades_and_lookup_by_alias() {
        let (_d, c) = ctx();
        let e =
            get_ma_school_screening_requirements(&c, ScreeningRequirementsArgs::default()).unwrap();
        assert_eq!(e.data.grade_table.len(), 14);
        let e = get_ma_school_screening_requirements(
            &c,
            ScreeningRequirementsArgs {
                screening: Some("scoliosis".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(e.data.screenings[0].id, "postural");
        assert!(
            get_ma_school_screening_requirements(
                &c,
                ScreeningRequirementsArgs {
                    screening: Some("qqqq".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            get_ma_school_screening_requirements(
                &c,
                ScreeningRequirementsArgs {
                    grade: Some("ungraded".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
