//! SCH-04 `build_individualized_health_plan`.
//!
//! Drafts an individualized health plan (IHP) and an emergency action plan
//! for diabetes, asthma, seizures, or food allergy/anaphylaxis from cited CDC
//! and DPH content, with nursing assessment items, goals, interventions,
//! medication-plan requirements (105 CMR 210.000), and suggested Section 504
//! accommodations for the 504 team.
//!
//! Template + local fill: the tool returns the plan template and its
//! placeholders. When pointed at a local file with student details, it fills
//! one plan per student and writes them to the output folder (the emergency
//! action plan in the family's language when the agency has a translation);
//! only counts are returned.

use std::collections::BTreeMap;

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::Document;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::localdata;
use crate::school::content::{self, CitedOut, Condition, PLANS, confidence_suffix, outs};
use crate::school::templates::{self, DocFormat, LanguageCount, TemplateResolver};

/// Arguments for `build_individualized_health_plan`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct HealthPlanArgs {
    /// Condition: diabetes, asthma, seizures (epilepsy), or food allergy
    /// (anaphylaxis).
    pub condition: String,
    /// Local fill: a CSV/XLSX in /data with one row per student (columns such
    /// as Student Name, Date of Birth, Grade, Parent Name, Parent Phone,
    /// Emergency Contact, Provider Name, Provider Phone, Allergies, Medications,
    /// Emergency Medication, and condition-specific fields). Plans are written
    /// to /data/output; nothing about the students is returned. Rows with a
    /// Condition column that names another condition are skipped.
    pub student_file: Option<String>,
    /// Sheet name for an XLSX file.
    pub sheet: Option<String>,
    /// docx (default) or pdf.
    pub format: Option<DocFormat>,
    /// Include suggested Section 504 accommodations (default true).
    pub include_504: Option<bool>,
    /// Write the blank English templates (no student data) to the output
    /// folder, for review or for the agency's translators (default false).
    pub write_templates: Option<bool>,
    /// School name for filled plans.
    pub school_name: Option<String>,
    /// Your name for filled plans.
    pub nurse_name: Option<String>,
    /// Your phone for filled plans.
    pub nurse_phone: Option<String>,
}

/// A plan section with cited statements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanSection {
    pub title: String,
    pub items: Vec<CitedOut>,
}

/// Documents written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlansWritten {
    pub folder: String,
    pub students: usize,
    pub files: Vec<String>,
    pub by_language: Vec<LanguageCount>,
    pub english_fallback: usize,
    /// Placeholders in an agency template that the tool does not fill.
    pub unknown_placeholders: Vec<String>,
    /// Placeholder -> number of students for whom it was blank.
    pub blank_fields: BTreeMap<String, usize>,
    /// Rows skipped because their Condition column named another condition.
    pub rows_skipped: usize,
}

/// Result of `build_individualized_health_plan`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HealthPlanResult {
    pub condition: String,
    pub summary: CitedOut,
    pub sections: Vec<PlanSection>,
    pub process: Vec<CitedOut>,
    pub legal: Vec<CitedOut>,
    /// Placeholders the IHP and emergency action plan use (fill from a local file).
    pub placeholders: Vec<String>,
    /// File names for agency translations of the emergency action plan.
    pub translation_kind: String,
    pub documents: Option<PlansWritten>,
    pub templates_written: Vec<String>,
}

/// Common placeholders and the column names that fill them.
const FIELDS: &[(&str, &[&str])] = &[
    (
        "student_name",
        &["student name", "name", "student", "full name"],
    ),
    (
        "date_of_birth",
        &["date of birth", "dob", "birth date", "birthdate"],
    ),
    ("grade", &["grade", "grade level"]),
    ("school", &["school", "school name"]),
    (
        "parent_name",
        &[
            "parent name",
            "parent guardian",
            "guardian",
            "parent",
            "guardian name",
        ],
    ),
    (
        "parent_phone",
        &["parent phone", "guardian phone", "phone", "home phone"],
    ),
    (
        "emergency_contact",
        &[
            "emergency contact",
            "alternate contact",
            "emergency contact phone",
        ],
    ),
    (
        "provider_name",
        &[
            "provider name",
            "provider",
            "physician",
            "doctor",
            "health care provider",
            "pcp",
        ],
    ),
    (
        "provider_phone",
        &["provider phone", "physician phone", "doctor phone"],
    ),
    ("diagnosis", &["diagnosis", "condition detail", "diagnoses"]),
    ("allergies", &["allergies", "allergy", "known allergies"]),
    (
        "medications",
        &[
            "medications",
            "daily medications",
            "school medications",
            "meds",
        ],
    ),
    (
        "emergency_medication",
        &[
            "emergency medication",
            "rescue medication",
            "emergency med",
            "rescue med",
        ],
    ),
    (
        "trained_staff",
        &["trained staff", "delegated staff", "staff trained"],
    ),
    ("plan_date", &["plan date", "start date", "date"]),
    ("review_date", &["review date", "renewal date"]),
    (
        "language",
        &[
            "home language",
            "primary language",
            "language",
            "family language",
        ],
    ),
];

/// Condition-specific placeholders.
fn extra_fields(id: &str) -> &'static [(&'static str, &'static [&'static str])] {
    match id {
        "diabetes" => &[
            (
                "target_range",
                &["target range", "glucose target", "bg target"],
            ),
            (
                "low_threshold",
                &["low threshold", "treat low below", "hypoglycemia threshold"],
            ),
            ("glucagon", &["glucagon", "glucagon type", "glucagon dose"]),
            ("insulin", &["insulin", "insulin plan", "insulin regimen"]),
        ],
        "asthma" => &[
            (
                "peak_flow_best",
                &["peak flow", "personal best", "peak flow best"],
            ),
            ("triggers", &["triggers", "asthma triggers"]),
        ],
        "seizures" => &[
            (
                "seizure_description",
                &[
                    "seizure description",
                    "seizure type",
                    "what seizures look like",
                ],
            ),
            (
                "rescue_instructions",
                &[
                    "rescue instructions",
                    "when to give rescue",
                    "rescue criteria",
                ],
            ),
            ("triggers", &["triggers", "seizure triggers"]),
        ],
        _ => &[
            ("allergens", &["allergens", "food allergens", "allergen"]),
            (
                "epinephrine",
                &[
                    "epinephrine",
                    "epinephrine dose",
                    "epi dose",
                    "auto-injector",
                ],
            ),
            ("asthma", &["asthma", "has asthma"]),
        ],
    }
}

fn find_condition(q: &str) -> Option<&'static Condition> {
    let n = phn_core::text::normalize(q);
    let c = &PLANS.conditions;
    c.iter()
        .find(|x| {
            phn_core::text::normalize(&x.id.replace('_', " ")) == n
                || phn_core::text::normalize(&x.name) == n
                || x.aliases.iter().any(|a| phn_core::text::normalize(a) == n)
        })
        .or_else(|| {
            c.iter().find(|x| {
                x.aliases
                    .iter()
                    .any(|a| n.contains(&phn_core::text::normalize(a)))
            })
        })
}

fn bullet(items: &[CitedOut]) -> String {
    items
        .iter()
        .map(|i| format!("- {} ({})\n", i.text, i.citation))
        .collect()
}

/// The IHP template (Markdown with placeholders).
pub fn ihp_template(cond: &Condition, include_504: bool) -> String {
    let src = &PLANS.sources;
    let mut s = format!(
        "# Individualized health plan: {}\n\n\
         | | |\n|---|---|\n\
         | Student | {{{{student_name}}}} |\n| Date of birth | {{{{date_of_birth}}}} |\n\
         | Grade and school | {{{{grade}}}}, {{{{school}}}} |\n\
         | Parent or guardian | {{{{parent_name}}}}, {{{{parent_phone}}}} |\n\
         | Emergency contact | {{{{emergency_contact}}}} |\n\
         | Health care provider | {{{{provider_name}}}}, {{{{provider_phone}}}} |\n\
         | Diagnosis (per provider) | {{{{diagnosis}}}} |\n| Allergies | {{{{allergies}}}} |\n\
         | Medications at school | {{{{medications}}}} |\n\
         | Emergency medication | {{{{emergency_medication}}}} |\n\
         | School nurse | {{{{nurse_name}}}}, {{{{nurse_phone}}}} |\n\
         | Plan date and review date | {{{{plan_date}}}}; review {{{{review_date}}}} |\n\n",
        cond.name
    );
    let extras = extra_fields(&cond.id);
    if !extras.is_empty() {
        s.push_str("| Condition details | |\n|---|---|\n");
        for (k, _) in extras {
            s.push_str(&format!("| {} | {{{{{k}}}}} |\n", k.replace('_', " ")));
        }
        s.push('\n');
    }
    s.push_str(&format!(
        "{} ({})\n\n",
        cond.summary.text.trim(),
        cond.summary.out(src).citation
    ));
    for (title, list) in [
        ("Nursing assessment", &cond.assessment),
        ("Goals", &cond.goals),
        ("Nursing interventions", &cond.interventions),
        ("Medications (105 CMR 210.000)", &cond.medications),
        ("Required plan elements", &cond.plan_elements),
    ] {
        s.push_str(&format!("## {title}\n\n{}\n", bullet(&outs(list, src))));
    }
    s.push_str("## Emergency action plan\n\nSee the attached emergency action plan.\n\n");
    if include_504 {
        s.push_str(&format!(
            "## Suggested accommodations for the Section 504 team\n\nThe 504 team decides \
             eligibility and accommodations; these are suggestions from the cited guidance.\n\n{}\n",
            bullet(&outs(&cond.accommodations, src))
        ));
    }
    s.push_str(
        "## Staff trained and communication\n\nStaff trained for this plan: {{trained_staff}}\n\n\
         ## Signatures\n\nParent or guardian: ____________________ Date: ________\n\n\
         School nurse: ____________________ Date: ________\n\n\
         Health care provider (orders attached): ____________________ Date: ________\n",
    );
    s
}

/// The emergency action plan template (family-facing; translatable).
pub fn eap_template(cond: &Condition) -> String {
    let src = &PLANS.sources;
    let mut s = format!(
        "# Emergency action plan: {}\n\n\
         | | |\n|---|---|\n| Student | {{{{student_name}}}} |\n| Grade | {{{{grade}}}} |\n\
         | Parent or guardian | {{{{parent_name}}}}, {{{{parent_phone}}}} |\n\
         | Emergency contact | {{{{emergency_contact}}}} |\n\
         | Health care provider | {{{{provider_name}}}}, {{{{provider_phone}}}} |\n\
         | Emergency medication (per order) | {{{{emergency_medication}}}} |\n\n",
        cond.name
    );
    s.push_str(&format!(
        "## Recognize\n\n{}\n## What to do\n\n{}\n## Call 911 when\n\n{}\n",
        bullet(&outs(&cond.recognition, src)),
        bullet(&outs(&cond.emergency_steps, src)),
        bullet(&outs(&cond.call_911, src))
    ));
    s.push_str(
        "Follow the health care provider's orders and this student's plan; the orders govern.\n\n\
         Health care provider: ____________________ Date: ________\n\n\
         Parent or guardian: ____________________ Date: ________\n\n\
         School nurse: {{nurse_name}}, {{nurse_phone}}\n",
    );
    s
}

fn kind_for(cond: &Condition) -> String {
    format!("{}_emergency_plan", cond.id)
}

fn column_for(headers: &[String], aliases: &[&str], key: &str) -> Option<usize> {
    let norm = |h: &str| phn_core::text::normalize(h);
    headers
        .iter()
        .position(|h| norm(h).replace(' ', "_") == key)
        .or_else(|| {
            headers
                .iter()
                .position(|h| aliases.contains(&norm(h).as_str()))
        })
}

/// Build an IHP and emergency action plan.
pub fn build_individualized_health_plan(
    ctx: &Context,
    args: HealthPlanArgs,
) -> Result<Envelope<HealthPlanResult>> {
    let c = &*PLANS;
    let src = &c.sources;
    let cond = find_condition(args.condition.trim()).ok_or_else(|| {
        PhnError::NotFound(format!(
            "no plan content for '{}'. Covered: {}",
            args.condition,
            c.conditions
                .iter()
                .map(|x| x.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })?;
    let include_504 = args.include_504.unwrap_or(true);
    let format = args.format.unwrap_or_default();
    let mut sections = vec![
        PlanSection {
            title: "Recognize".into(),
            items: outs(&cond.recognition, src),
        },
        PlanSection {
            title: "Emergency steps".into(),
            items: outs(&cond.emergency_steps, src),
        },
        PlanSection {
            title: "Call 911 when".into(),
            items: outs(&cond.call_911, src),
        },
        PlanSection {
            title: "Nursing assessment".into(),
            items: outs(&cond.assessment, src),
        },
        PlanSection {
            title: "Goals".into(),
            items: outs(&cond.goals, src),
        },
        PlanSection {
            title: "Nursing interventions".into(),
            items: outs(&cond.interventions, src),
        },
        PlanSection {
            title: "Medications".into(),
            items: outs(&cond.medications, src),
        },
        PlanSection {
            title: "Required plan elements".into(),
            items: outs(&cond.plan_elements, src),
        },
    ];
    if include_504 {
        sections.push(PlanSection {
            title: "Suggested Section 504 accommodations".into(),
            items: outs(&cond.accommodations, src),
        });
    }
    sections.push(PlanSection {
        title: "Resources for families".into(),
        items: outs(&cond.family_resources, src),
    });
    let mut placeholders: Vec<String> = FIELDS.iter().map(|(k, _)| k.to_string()).collect();
    placeholders.extend(extra_fields(&cond.id).iter().map(|(k, _)| k.to_string()));
    placeholders.extend(["nurse_name".to_string(), "nurse_phone".to_string()]);
    let ihp = ihp_template(cond, include_504);
    let eap = eap_template(cond);
    let kind = kind_for(cond);

    let mut files_written = Vec::new();
    let mut templates_written = Vec::new();
    if args.write_templates.unwrap_or(false) {
        let (dir, _) =
            templates::output_folder(ctx, &format!("health_plan_templates_{}", cond.id))?;
        templates_written.push(templates::write_in(
            ctx,
            &dir,
            &format!("ihp_{}.md", cond.id),
            ihp.as_bytes(),
        )?);
        templates_written.push(templates::write_in(
            ctx,
            &dir,
            &format!("template_english_{kind}.md"),
            eap.as_bytes(),
        )?);
        let blank = BTreeMap::new();
        let (f1, _) = templates::fill(&ihp, &blank, "________");
        let d1 = templates::document(ctx, &f1, "en-US", "Individualized health plan");
        templates_written.push(templates::write_in(
            ctx,
            &dir,
            &format!("ihp_{}.{}", cond.id, format.ext()),
            &format.render(&d1)?,
        )?);
        files_written.extend(templates_written.clone());
    }

    let mut identifiers = Vec::new();
    let mut local_label = None;
    let documents = match args
        .student_file
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        Some(path) => {
            let lt = localdata::load(ctx, path, args.sheet.as_deref())?;
            identifiers = lt.identifier_categories();
            local_label = Some(lt.label.clone());
            let h = &lt.table.headers;
            let mut cols: Vec<(String, usize)> = Vec::new();
            for (k, aliases) in FIELDS.iter().chain(extra_fields(&cond.id).iter()) {
                if let Some(i) = column_for(h, aliases, k) {
                    cols.push((k.to_string(), i));
                }
            }
            if !cols.iter().any(|(k, _)| k == "student_name") {
                let first = column_for(h, &["first name", "first"], "first_name");
                let last = column_for(h, &["last name", "last", "surname"], "last_name");
                if first.is_none() && last.is_none() {
                    return Err(PhnError::InvalidInput(format!(
                        "the student file needs a 'Student Name' column (or 'First Name' and 'Last Name'). Columns found: {}",
                        h.join(", ")
                    )));
                }
            }
            let cond_col = column_for(
                h,
                &["condition", "health condition", "plan type"],
                "condition",
            );
            let (dir, shown) = templates::output_folder(ctx, &format!("health_plans_{}", cond.id))?;
            let mut resolver = TemplateResolver::new(ctx, &kind, eap.clone());
            let mut blank_fields: BTreeMap<String, usize> = BTreeMap::new();
            let mut fallback = 0usize;
            let mut unknown_placeholders: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            let mut by_lang: BTreeMap<(String, bool), usize> = BTreeMap::new();
            let mut files = Vec::new();
            let mut n = 0usize;
            let mut skipped = 0usize;
            for row in &lt.table.rows {
                if row.iter().all(|x| x.trim().is_empty()) {
                    continue;
                }
                if let Some(ci) = cond_col {
                    let v = row.get(ci).map(String::as_str).unwrap_or("");
                    if !v.trim().is_empty()
                        && find_condition(v).map(|x| x.id.as_str()) != Some(cond.id.as_str())
                    {
                        skipped += 1;
                        continue;
                    }
                }
                n += 1;
                let mut values: BTreeMap<String, String> = BTreeMap::new();
                for (k, i) in &cols {
                    values.insert(k.clone(), row.get(*i).cloned().unwrap_or_default());
                }
                if !values.contains_key("student_name") {
                    let f = column_for(h, &["first name", "first"], "first_name")
                        .and_then(|i| row.get(i).cloned())
                        .unwrap_or_default();
                    let l = column_for(h, &["last name", "last", "surname"], "last_name")
                        .and_then(|i| row.get(i).cloned())
                        .unwrap_or_default();
                    values.insert("student_name".into(), format!("{f} {l}").trim().to_string());
                }
                if values.get("school").is_none_or(|v| v.trim().is_empty())
                    && let Some(sn) = &args.school_name
                {
                    values.insert("school".into(), sn.clone());
                }
                values.insert(
                    "nurse_name".into(),
                    args.nurse_name.clone().unwrap_or_default(),
                );
                values.insert(
                    "nurse_phone".into(),
                    args.nurse_phone.clone().unwrap_or_default(),
                );
                for p in &placeholders {
                    if values.get(p).is_none_or(|v| v.trim().is_empty()) && p != "language" {
                        *blank_fields.entry(p.clone()).or_default() += 1;
                    }
                }
                let lang = values.get("language").cloned().unwrap_or_default();
                let (ihp_filled, _) = templates::fill(&ihp, &values, "________");
                let chosen = resolver.choose(&lang);
                if !chosen.translated && !templates::is_english(&lang) {
                    fallback += 1;
                }
                *by_lang
                    .entry((chosen.language.clone(), chosen.translated))
                    .or_default() += 1;
                let (eap_filled, unk) = templates::fill(&chosen.markdown, &values, "________");
                unknown_placeholders.extend(unk);
                let d1 =
                    templates::document(ctx, &ihp_filled, "en-US", "Individualized health plan");
                let d2 =
                    templates::document(ctx, &eap_filled, &chosen.tag, "Emergency action plan");
                let both: Document = templates::combine(
                    &format!("Health plan: {}", cond.name),
                    "en-US",
                    vec![d1, d2],
                    ctx,
                );
                let bytes = format.render(&both)?;
                files.push(templates::write_in(
                    ctx,
                    &dir,
                    &format!("{}_plan_{:03}.{}", cond.id, n, format.ext()),
                    &bytes,
                )?);
            }
            files.push(templates::write_in(
                ctx,
                &dir,
                &format!("template_english_{kind}.md"),
                eap.as_bytes(),
            )?);
            files_written.extend(files.clone());
            Some(PlansWritten {
                folder: shown,
                students: n,
                files,
                by_language: by_lang
                    .into_iter()
                    .map(
                        |((language, agency_translation), documents)| LanguageCount {
                            language,
                            documents,
                            agency_translation,
                        },
                    )
                    .collect(),
                english_fallback: fallback,
                unknown_placeholders: unknown_placeholders.into_iter().collect(),
                blank_fields,
                rows_skipped: skipped,
            })
        }
        None => None,
    };

    // ---- Markdown
    let mut s = format!(
        "**Individualized health plan and emergency action plan: {}**\n\n",
        cond.name
    );
    let summary = cond.summary.out(src);
    s.push_str(&format!("{} ({})\n", summary.text, summary.citation));
    for sec in &sections {
        s.push_str(&format!("\n**{}**\n\n", sec.title));
        for i in &sec.items {
            s.push_str(&format!(
                "- {} ({}){}\n",
                i.text,
                i.citation,
                confidence_suffix(i.confidence)
            ));
        }
    }
    match &documents {
        Some(d) => {
            s.push_str(&format!(
                "\n**Plans written:** {} student plan(s) (IHP plus emergency action plan) in `{}`{}.\n",
                d.students,
                d.folder,
                if d.rows_skipped > 0 {
                    format!("; {} row(s) for other conditions skipped", d.rows_skipped)
                } else {
                    String::new()
                }
            ));
            if !d.blank_fields.is_empty() {
                s.push_str(&format!(
                    "Fields left blank for some students (complete them by hand): {}.\n",
                    d.blank_fields
                        .iter()
                        .map(|(k, v)| format!("{k} ({v})"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !d.unknown_placeholders.is_empty() {
                s.push_str(&format!(
                    "Your agency template uses placeholder(s) the tool does not fill (left as [name]): {}.\n",
                    d.unknown_placeholders.join(", ")
                ));
            }
            if d.english_fallback > 0 {
                s.push_str(&format!(
                    "{} emergency action plan(s) are in English because the agency has no translation for the family's language. {}\n",
                    d.english_fallback,
                    templates::translation_help(&[&kind])
                ));
            }
        }
        None => s.push_str(&format!(
            "\n**To fill plans locally:** put one row per student in a CSV or Excel file in /data with columns for: {}. Then ask again with student_file; the plans are written to /data/output and nothing about the students comes back here. Do not type a student's details into the chat.\n",
            placeholders.join(", ")
        )),
    }
    if !templates_written.is_empty() {
        s.push_str(&format!(
            "\nBlank templates written: {}.\n",
            templates_written.join(", ")
        ));
    }

    let mut caveats = content::review(&[content::HEALTH_PLANS]);
    caveats.push(
        "This is a nursing plan template organized from cited CDC and DPH guidance. The student's \
         health care provider's orders govern treatment; the nurse sets the goals; the Section 504 \
         team decides eligibility and accommodations."
            .into(),
    );
    caveats.push(
        "Emergency action plan forms from the Epilepsy Foundation, FARE, and NIDDK could not be \
         retrieved and are cited by name only; your district may require a specific form."
            .into(),
    );
    let mut keys: Vec<&str> = vec![cond.summary.source.as_str()];
    for list in [
        &cond.recognition,
        &cond.emergency_steps,
        &cond.call_911,
        &cond.goals,
        &cond.interventions,
        &cond.accommodations,
        &cond.medications,
        &cond.plan_elements,
    ] {
        keys.extend(list.iter().map(|x| x.source.as_str()));
    }
    let mut sources = Vec::new();
    if let Some(l) = local_label {
        sources.push(Source::local_file(l));
    }
    sources.extend(content::sources_for(src, keys));
    let privacy = if documents.is_some() || !files_written.is_empty() {
        Privacy {
            row_level_data: false,
            suppression_applied: false,
            suppression_threshold: None,
            identifiers_detected: identifiers,
            files_written,
            notes: vec![
                "Filled plans are in the output folder; only counts and file names are returned."
                    .into(),
            ],
        }
    } else {
        Privacy::none()
    };
    Ok(Envelope::new(
        s,
        HealthPlanResult {
            condition: cond.name.clone(),
            summary,
            sections,
            process: outs(&c.process, src),
            legal: outs(&c.legal, src),
            placeholders,
            translation_kind: kind,
            documents,
            templates_written,
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
    fn every_condition_has_a_cited_template() {
        let (_d, c) = ctx();
        for q in ["diabetes", "Asthma", "epilepsy", "peanut allergy"] {
            let e = build_individualized_health_plan(
                &c,
                HealthPlanArgs {
                    condition: q.into(),
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(
                e.data
                    .sections
                    .iter()
                    .all(|s| s.items.iter().all(|i| !i.citation.is_empty()))
            );
            assert!(e.data.sections.iter().any(|s| s.title.contains("504")));
            assert!(e.summary.contains("student_file"));
            assert!(e.notice.is_some());
        }
        assert!(
            build_individualized_health_plan(
                &c,
                HealthPlanArgs {
                    condition: "gout".into(),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn local_fill_writes_plans_and_returns_counts_only() {
        let (d, c) = ctx();
        std::fs::write(
            d.path().join("data/plans.csv"),
            "Student Name,Date of Birth,Grade,Parent Name,Parent Phone,Home Language,Condition,Allergens,Epinephrine\n\
Quill Zorvath,2015-04-01,5,Dara Zorvath,617-555-0142,Spanish,food allergy,peanut,0.3 mg auto-injector\n\
Oro Brembly,2014-02-01,6,Lio Brembly,617-555-0143,English,asthma,,\n",
        )
        .unwrap();
        let e = build_individualized_health_plan(
            &c,
            HealthPlanArgs {
                condition: "food allergy".into(),
                student_file: Some("plans.csv".into()),
                write_templates: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let p = e.data.documents.as_ref().unwrap();
        assert_eq!((p.students, p.rows_skipped, p.english_fallback), (1, 1, 1));
        let all = serde_json::to_string(&e).unwrap();
        for v in ["Zorvath", "Quill", "617-555-0142", "2015-04-01", "peanut"] {
            assert!(!all.contains(v), "{v} leaked");
        }
        assert!(d.path().join("data").join(&p.files[0]).exists());
        assert!(!e.data.templates_written.is_empty());
    }
}
