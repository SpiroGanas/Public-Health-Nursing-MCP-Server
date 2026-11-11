//! EPR-03 `functional_needs_shelter_intake`: a CMIST shelter health intake
//! (communication, maintaining health, independence, support and safety,
//! transportation) as a blank form and structured template, filled locally
//! from a file of registrants (written to /data/output only), plus planning
//! estimates of support needs, supplies, and staffing for an expected
//! shelter population.

use std::collections::BTreeMap;

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::area::{self, Area};
use super::content::{self, Assumption, CitedOut, CmistDomain, Shelter};
use super::population::{self, PopulationProfile};
use crate::context::Context;
use crate::md;

/// Arguments for `functional_needs_shelter_intake`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ShelterArgs {
    /// The city, town, or county the shelter serves (default: your saved town; local rates
    /// are used for the planning estimates).
    pub area: Option<String>,
    /// Expected number of shelter residents, for planning estimates of needs, supplies, and staff.
    pub expected_residents: Option<u32>,
    /// Blank intake form to write to /data/output: "docx" (default), "xlsx", "both", or "none".
    pub blank_form: Option<String>,
    /// A CSV or XLSX file in /data with one row per registrant (name and other details plus
    /// answers in columns named like the questions, e.g. interpreter_needed, mobility_aid).
    /// Filled forms are written to /data/output; only suppressed counts are returned.
    pub intake_path: Option<String>,
    /// Override: residents per health services staff member per shift.
    pub residents_per_health_staff: Option<f64>,
    /// Override: residents with functional needs per personal-assistance staff member per shift.
    pub fnss_residents_per_support_staff: Option<f64>,
    /// Override: residents with limited English per interpreter per shift.
    pub lep_residents_per_interpreter: Option<f64>,
}

/// A planning estimate for one need.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NeedEstimate {
    /// C, M, I, S, or T.
    pub domain: String,
    pub need: String,
    /// Local rate used (percent).
    pub rate_percent: f64,
    /// Expected residents with the need (rate × residents, rounded up).
    pub expected_residents: u32,
    /// What it means for supplies and support.
    pub plan_for: String,
    pub rate_source: String,
}

/// A staffing line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StaffEstimate {
    pub role: String,
    pub per_shift: u32,
    pub how: String,
    /// "tool_default" or "your_input".
    pub basis: String,
}

/// Counts from a batch of intake records (suppressed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IntakeBatch {
    pub file: String,
    pub registrants: usize,
    /// Question → "yes" count ("<5" when suppressed).
    pub yes_counts: BTreeMap<String, String>,
    /// Preferred language → count (suppressed).
    pub languages: BTreeMap<String, String>,
    /// Forms written in an agency translation.
    pub forms_in_agency_translation: usize,
    /// Forms written in English because no agency translation was found for the language.
    pub forms_in_english_fallback: usize,
    pub columns_matched: Vec<String>,
    pub output_file: String,
}

/// Result of `functional_needs_shelter_intake`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShelterResult {
    pub area: Area,
    pub cmist_definition: String,
    /// The intake template: CMIST domains and questions (column names for a local file are the question ids).
    pub domains: Vec<CmistDomain>,
    pub dispositions: Vec<String>,
    pub mass_care_tasks: Vec<CitedOut>,
    pub electric_equipment: Vec<String>,
    pub expected_residents: Option<u32>,
    pub needs: Vec<NeedEstimate>,
    pub staffing: Vec<StaffEstimate>,
    pub staffing_assumptions: Vec<Assumption>,
    pub batch: Option<IntakeBatch>,
    pub blank_forms: Vec<String>,
    /// Header fields a local file may also have (written to forms only).
    pub person_fields: Vec<String>,
    pub translation_folder: String,
}

const PERSON_FIELDS: &[(&str, &[&str])] = &[
    (
        "Name",
        &[
            "name",
            "full name",
            "resident name",
            "registrant",
            "first name",
            "last name",
        ],
    ),
    ("Date of birth", &["date of birth", "dob", "birth date"]),
    ("Phone", &["phone", "telephone", "cell"]),
    (
        "Home address",
        &["address", "home address", "street address"],
    ),
    (
        "Emergency contact",
        &["emergency contact", "contact person", "next of kin"],
    ),
    (
        "Arrival",
        &["arrival", "arrival date", "date", "registered"],
    ),
    (
        "Preferred language",
        &["preferred language", "language", "preferred_language"],
    ),
];

fn rate(p: &PopulationProfile, id: &str) -> Option<(f64, String)> {
    p.demographics
        .iter()
        .chain(p.health.iter())
        .find(|i| i.id == id)
        .and_then(|i| i.percent.map(|v| (v, i.source.clone())))
}

fn blank_doc(ctx: &Context, c: &Shelter, values: Option<&BTreeMap<String, String>>) -> Document {
    let v = |k: &str| values.and_then(|m| m.get(k)).cloned().unwrap_or_default();
    let mut blocks = vec![
        Block::Paragraph(vec![Inline::plain(
            "Shelter health intake (CMIST). Ask every person registering; record needs, not diagnoses. Keep completed forms confidential.",
        )]),
        Block::Table(TableBlock {
            caption: Some("Registrant".into()),
            headers: vec!["Field".into(), "Entry".into()],
            rows: PERSON_FIELDS
                .iter()
                .map(|(f, _)| vec![f.to_string(), v(f)])
                .chain([vec![
                    "Shelter and intake staff".into(),
                    v("Shelter and intake staff"),
                ]])
                .collect(),
        }),
    ];
    for d in &c.domains {
        blocks.push(Block::Heading {
            level: 2,
            text: format!("{} — {}", d.letter, d.label),
        });
        blocks.push(Block::Table(TableBlock {
            caption: Some(d.label.clone()),
            headers: vec!["Question".into(), "Answer".into(), "Notes or action".into()],
            rows: d
                .questions
                .iter()
                .map(|q| {
                    let a = v(&q.id);
                    vec![
                        q.text.clone(),
                        if a.is_empty() && q.kind == "yes_no" && values.is_none() {
                            "Yes / No".into()
                        } else {
                            a
                        },
                        String::new(),
                    ]
                })
                .collect(),
        }));
    }
    blocks.push(Block::Heading {
        level: 2,
        text: "Disposition".into(),
    });
    blocks.push(Block::List {
        ordered: false,
        items: c
            .dispositions
            .iter()
            .map(|d| vec![Inline::plain(format!("☐ {d}"))])
            .collect(),
    });
    if let Some(other) = values.and_then(|m| m.get("__other")) {
        blocks.push(Block::Paragraph(vec![Inline::plain(format!(
            "Other information from the registration file: {other}"
        ))]));
    }
    Document {
        title: "Shelter health intake (CMIST)".into(),
        subtitle: Some(
            "Communication, Maintaining health, Independence, Support and safety, Transportation"
                .into(),
        ),
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

/// Replace `{{ key }}` placeholders with values.
fn fill_placeholders(text: &str, value: impl Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("}}") {
            Some(end) => {
                out.push_str(&value(rest[start + 2..start + 2 + end].trim()));
                rest = &rest[start + 2 + end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn suppress(n: usize, threshold: u32) -> String {
    if n > 0 && (n as u32) < threshold {
        format!("<{threshold}")
    } else {
        n.to_string()
    }
}

/// CMIST intake template, local fill, and planning estimates.
pub async fn functional_needs_shelter_intake(
    ctx: &Context,
    args: ShelterArgs,
) -> Result<Envelope<ShelterResult>> {
    let c: Shelter = content::load(content::SHELTER)?;
    let area = area::resolve(ctx, args.area.as_deref(), true)?;
    let mut caveats = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    let mut files_written = Vec::new();
    let mut identifiers = Vec::new();
    if let Some(n) = args.expected_residents
        && !(1..=100_000).contains(&n)
    {
        return Err(PhnError::InvalidInput(
            "expected_residents must be between 1 and 100,000".into(),
        ));
    }

    // Planning estimates.
    let mut needs = Vec::new();
    let mut staffing = Vec::new();
    let mut staffing_assumptions = Vec::new();
    if let Some(n) = args.expected_residents {
        let (profile, psrc, pcav) = population::profile(ctx, &area, true).await?;
        sources.extend(psrc);
        caveats.extend(pcav);
        let nf = n as f64;
        let mut push = |domain: &str, need: &str, r: Option<(f64, String)>, plan: &str| {
            if let Some((pct, src)) = r {
                needs.push(NeedEstimate {
                    domain: domain.into(),
                    need: need.into(),
                    rate_percent: pct,
                    expected_residents: (nf * pct / 100.0).ceil() as u32,
                    plan_for: plan.into(),
                    rate_source: src,
                });
            }
        };
        push(
            "C",
            "Limited English proficiency",
            rate(&profile, "lep_pct"),
            "Interpreters (in person or remote) and translated signs and forms",
        );
        push(
            "C",
            "Hearing disability (adults)",
            rate(&profile, "HEARING"),
            "ASL interpreters, captioning, visual alerts, hearing aid batteries",
        );
        push(
            "C",
            "Vision disability (adults)",
            rate(&profile, "VISION"),
            "Large print and braille, guided orientation, clear pathways",
        );
        push(
            "C",
            "Cognitive disability (adults)",
            rate(&profile, "COGNITION"),
            "Plain-language instructions, quiet space, supervision plans",
        );
        push(
            "M",
            "Any disability",
            rate(&profile, "disability_pct"),
            "Functional needs support services in the general population shelter",
        );
        push(
            "M",
            "COPD (adults)",
            rate(&profile, "COPD"),
            "Medication refills, oxygen, smoke-free and well-ventilated space",
        );
        push(
            "M",
            "Diabetes (adults)",
            rate(&profile, "DIABETES"),
            "Refrigeration for insulin, glucose supplies, regular meals and snacks",
        );
        push(
            "I",
            "Mobility disability (adults)",
            rate(&profile, "MOBILITY"),
            "Accessible cots, routes, toilets and showers; mobility aids",
        );
        push(
            "I",
            "Self-care disability (adults)",
            rate(&profile, "SELFCARE"),
            "Personal assistance with bathing, dressing, toileting",
        );
        push(
            "S",
            "Independent living disability (adults)",
            rate(&profile, "INDEPLIVE"),
            "Support for errands, appointments, and decisions; caseworker follow-up",
        );
        push(
            "S",
            "Adults 65 and older",
            rate(&profile, "age_65plus_pct"),
            "Checks for heat or cold stress, falls, medication needs",
        );
        push(
            "S",
            "Children under 5",
            rate(&profile, "age_under5_pct"),
            "Cribs, infant formula and diapers, child-safe space",
        );
        push(
            "T",
            "Households without a vehicle",
            rate(&profile, "no_vehicle_pct"),
            "Accessible transportation to the shelter, dialysis, and appointments",
        );
        if let (Some(e), Some(pop)) = (&profile.electricity_dependent, profile.population)
            && pop > 0.0
        {
            let pct = (e.electricity_dependent / pop * 1000.0).round() / 10.0;
            needs.push(NeedEstimate {
                domain: "M".into(),
                need: "Relies on electricity-dependent medical equipment (Medicare beneficiaries)".into(),
                rate_percent: pct,
                expected_residents: (nf * pct / 100.0).ceil() as u32,
                plan_for: "Grounded outlets on backup power or charging stations near cots; oxygen supply; spare batteries".into(),
                rate_source: format!("HHS emPOWER ÷ ACS population ({})", e.mode),
            });
        }
        let val = |id: &str, over: Option<f64>| -> Result<(f64, Assumption)> {
            let a = c
                .staffing_defaults
                .iter()
                .find(|a| a.id == id)
                .ok_or_else(|| PhnError::Internal(format!("staffing default {id} missing")))?;
            match over {
                Some(v) if !(v.is_finite() && v > 0.0 && v <= 10_000.0) => Err(
                    PhnError::InvalidInput(format!("{id} must be a positive number")),
                ),
                Some(v) => Ok((
                    v,
                    Assumption {
                        value: v,
                        basis: "your_input".into(),
                        note: format!("You set this (tool default {}).", a.value),
                        ..a.clone()
                    },
                )),
                None => Ok((a.value, a.clone())),
            }
        };
        let (r1, a1) = val(
            "residents_per_health_staff",
            args.residents_per_health_staff,
        )?;
        let (r2, a2) = val(
            "fnss_residents_per_support_staff",
            args.fnss_residents_per_support_staff,
        )?;
        let (r3, a3) = val(
            "lep_residents_per_interpreter",
            args.lep_residents_per_interpreter,
        )?;
        let adl = needs
            .iter()
            .find(|x| x.need.starts_with("Self-care"))
            .map(|x| x.expected_residents)
            .unwrap_or(0);
        let lep = needs
            .iter()
            .find(|x| x.need.starts_with("Limited English"))
            .map(|x| x.expected_residents)
            .unwrap_or(0);
        staffing.push(StaffEstimate {
            role: "Health services (nurses and other licensed staff)".into(),
            per_shift: ((nf / r1).ceil() as u32).max(1),
            how: format!("{n} residents ÷ {r1}"),
            basis: a1.basis.clone(),
        });
        staffing.push(StaffEstimate {
            role: "Personal assistance / functional needs support".into(),
            per_shift: (adl as f64 / r2).ceil() as u32,
            how: format!("{adl} residents needing help with self-care ÷ {r2}"),
            basis: a2.basis.clone(),
        });
        staffing.push(StaffEstimate {
            role: "Interpreters (in person or remote)".into(),
            per_shift: (lep as f64 / r3).ceil() as u32,
            how: format!("{lep} residents with limited English ÷ {r3}"),
            basis: a3.basis.clone(),
        });
        staffing_assumptions = vec![a1, a2, a3];
        caveats.push(
            "Need estimates apply the community's rates to the shelter population; people who \
             come to shelters often have more needs than the community average (fewer resources \
             and more health conditions), so treat these as minimums."
                .into(),
        );
    }

    // Blank form.
    let mut blank_forms = Vec::new();
    let which = args
        .blank_form
        .as_deref()
        .unwrap_or("docx")
        .trim()
        .to_ascii_lowercase();
    if !["docx", "xlsx", "both", "none"].contains(&which.as_str()) {
        return Err(PhnError::InvalidInput(
            "blank_form must be docx, xlsx, both, or none".into(),
        ));
    }
    if which != "none" {
        let doc = blank_doc(ctx, &c, None);
        let sandbox = ctx.sandbox()?;
        for (fmt, on) in [
            ("docx", which == "docx" || which == "both"),
            ("xlsx", which == "xlsx" || which == "both"),
        ] {
            if !on {
                continue;
            }
            let bytes = if fmt == "docx" {
                phn_docs::docx::to_docx(&doc)?
            } else {
                phn_docs::xlsx::to_xlsx(&doc)?
            };
            let path = sandbox.new_output_file(&format!("cmist_shelter_intake_blank.{fmt}"))?;
            std::fs::write(&path, bytes)?;
            let disp = sandbox.display_path(&path);
            files_written.push(disp.clone());
            blank_forms.push(disp);
        }
    }

    // Local fill.
    let threshold = ctx.suppression_threshold();
    let mut batch = None;
    if let Some(p) = &args.intake_path {
        let lt = crate::localdata::load(ctx, p, None)?;
        identifiers = lt.identifier_categories();
        let norm = |s: &str| phn_core::text::normalize(s).replace(' ', "_");
        let headers: Vec<String> = lt.table.headers.iter().map(|h| norm(h)).collect();
        let mut qcols: Vec<(String, usize)> = Vec::new();
        for d in &c.domains {
            for q in &d.questions {
                if let Some(i) = headers.iter().position(|h| *h == q.id) {
                    qcols.push((q.id.clone(), i));
                }
            }
        }
        let mut pcols: Vec<(String, usize)> = Vec::new();
        for (field, aliases) in PERSON_FIELDS {
            if let Some(i) = headers
                .iter()
                .position(|h| aliases.iter().any(|a| norm(a) == *h))
            {
                pcols.push((field.to_string(), i));
            }
        }
        if qcols.is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "'{}' has no columns named like the intake questions (for example interpreter_needed, \
                 electric_equipment, mobility_aid). Columns found: {}",
                lt.label,
                lt.table.headers.len()
            )));
        }
        let used: Vec<usize> = qcols.iter().chain(pcols.iter()).map(|(_, i)| *i).collect();
        let lang_col = pcols
            .iter()
            .find(|(f, _)| f == "Preferred language")
            .map(|(_, i)| *i)
            .or_else(|| {
                qcols
                    .iter()
                    .find(|(f, _)| f == "preferred_language")
                    .map(|(_, i)| *i)
            });
        let mut yes: BTreeMap<String, usize> = BTreeMap::new();
        let mut langs: BTreeMap<String, usize> = BTreeMap::new();
        let (mut translated, mut fallback) = (0usize, 0usize);
        let mut out = Document {
            title: "Shelter health intake (CMIST) — completed forms".into(),
            subtitle: None,
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks: Vec::new(),
        };
        let templates_dir = ctx.config.paths.data_dir.join("templates");
        let mut registrants = 0usize;
        for row in &lt.table.rows {
            if row.iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            registrants += 1;
            let mut values: BTreeMap<String, String> = BTreeMap::new();
            for (f, i) in qcols.iter().chain(pcols.iter()) {
                let v = row
                    .get(*i)
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                if crate::localdata::truthy(&v) == Some(true)
                    && !f.chars().next().is_some_and(|c| c.is_uppercase())
                {
                    *yes.entry(f.clone()).or_default() += 1;
                }
                values.insert(f.clone(), v);
            }
            let other: Vec<String> = lt
                .table
                .headers
                .iter()
                .enumerate()
                .filter(|(i, _)| !used.contains(i))
                .filter_map(|(i, h)| {
                    row.get(i)
                        .filter(|v| !v.trim().is_empty())
                        .map(|v| format!("{h}: {}", v.trim()))
                })
                .collect();
            if !other.is_empty() {
                values.insert("__other".into(), other.join("; "));
            }
            let lang = lang_col
                .and_then(|i| row.get(i))
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| "Not recorded".into());
            *langs.entry(lang.clone()).or_default() += 1;
            // The same folder names as the other tools ("Haitian Creole" →
            // haitian_creole); the helper keeps only letters, digits, - and _, so a
            // cell value can never point outside /data/templates.
            let tpl = crate::core::practice::language_folder(&lang)
                .map(|folder| templates_dir.join(folder).join("cmist_intake.md"));
            let is_english = lang.eq_ignore_ascii_case("english") || lang == "Not recorded";
            let filled = match tpl
                .filter(|_| !is_english)
                .and_then(|path| std::fs::read_to_string(path).ok())
            {
                Some(text) => {
                    translated += 1;
                    let replaced = fill_placeholders(&text, |key| {
                        values
                            .get(key)
                            .or_else(|| values.get(&norm(key)))
                            .cloned()
                            .unwrap_or_default()
                    });
                    let mut d = blank_doc(ctx, &c, Some(&values));
                    d.blocks = phn_docs::model::from_markdown(&replaced);
                    d
                }
                None => {
                    if !is_english {
                        fallback += 1;
                    }
                    blank_doc(ctx, &c, Some(&values))
                }
            };
            if !out.blocks.is_empty() {
                out.blocks.push(Block::PageBreak);
            }
            out.blocks.push(Block::Heading {
                level: 1,
                text: format!("Registrant {registrants}"),
            });
            out.blocks.extend(filled.blocks);
        }
        let bytes = phn_docs::docx::to_docx(&out)?;
        let sandbox = ctx.sandbox()?;
        let path = sandbox.new_output_file("cmist_shelter_intake_forms.docx")?;
        std::fs::write(&path, bytes)?;
        let disp = sandbox.display_path(&path);
        files_written.push(disp.clone());
        let mut yes_counts = BTreeMap::new();
        for (id, _) in &qcols {
            if c.domains
                .iter()
                .flat_map(|d| d.questions.iter())
                .any(|q| &q.id == id && q.kind == "yes_no")
            {
                yes_counts.insert(id.clone(), suppress(*yes.get(id).unwrap_or(&0), threshold));
            }
        }
        // Languages: suppress small groups and keep only plain language names.
        let languages: BTreeMap<String, String> = langs
            .iter()
            .map(|(l, n)| {
                let label = if l.chars().all(|c| c.is_alphabetic() || c == ' ' || c == '-')
                    && l.len() <= 30
                {
                    l.clone()
                } else {
                    "Other".into()
                };
                (label, suppress(*n, threshold))
            })
            .collect();
        if fallback > 0 {
            caveats.push(format!(
                "{fallback} form(s) were written in English because no agency translation was found \
                 in /data/templates/<language>/cmist_intake.md for the person's language; arrange \
                 interpretation."
            ));
        }
        batch = Some(IntakeBatch {
            file: lt.label.clone(),
            registrants,
            yes_counts,
            languages,
            forms_in_agency_translation: translated,
            forms_in_english_fallback: fallback,
            columns_matched: qcols
                .iter()
                .map(|(q, _)| q.clone())
                .chain(pcols.iter().map(|(p, _)| p.clone()))
                .collect(),
            output_file: disp,
        });
        sources.push(lt.source());
    }

    // Summary.
    let mut s = format!(
        "**Shelter health intake (CMIST) — {}**\n\n{}\n\n",
        area.name, c.cmist_definition.text
    );
    for d in &c.domains {
        s.push_str(&format!(
            "- **{} — {}:** {}\n",
            d.letter,
            d.label,
            d.questions
                .iter()
                .map(|q| q.text.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    if !needs.is_empty() {
        s.push_str(&format!(
            "\n**Planning estimates for {} residents** (local rates for {})\n\n",
            args.expected_residents.unwrap_or_default(),
            area.name
        ));
        let rows: Vec<Vec<String>> = needs
            .iter()
            .map(|n| {
                vec![
                    n.domain.clone(),
                    n.need.clone(),
                    format!("{}%", n.rate_percent),
                    n.expected_residents.to_string(),
                    n.plan_for.clone(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "CMIST",
                "Need",
                "Local rate",
                "Expected residents",
                "Plan for",
            ],
            &rows,
        ));
        s.push_str("\n**Staff per shift**\n\n");
        let rows: Vec<Vec<String>> = staffing
            .iter()
            .map(|x| {
                vec![
                    x.role.clone(),
                    x.per_shift.to_string(),
                    x.how.clone(),
                    x.basis.replace('_', " "),
                ]
            })
            .collect();
        s.push_str(&md::table(&["Role", "Per shift", "How", "Basis"], &rows));
    }
    if let Some(b) = &batch {
        s.push_str(&format!(
            "\n**Registrations:** {} record(s) from `{}`; completed forms written to `{}` (not shown here). \
             Forms in an agency translation: {}; English fallback: {}.\n\n",
            b.registrants, b.file, b.output_file, b.forms_in_agency_translation, b.forms_in_english_fallback
        ));
        let rows: Vec<Vec<String>> = b
            .yes_counts
            .iter()
            .map(|(k, v)| vec![k.clone(), v.clone()])
            .collect();
        s.push_str(&md::table(&["Need (answered yes)", "Registrants"], &rows));
        let rows: Vec<Vec<String>> = b
            .languages
            .iter()
            .map(|(k, v)| vec![k.clone(), v.clone()])
            .collect();
        s.push_str(&md::table(&["Preferred language", "Registrants"], &rows));
        s.push_str(&format!("`<{threshold}` = suppressed small count.\n"));
    }
    if !blank_forms.is_empty() {
        s.push_str(&format!(
            "\nBlank intake form written to {}.\n",
            blank_forms
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(" and ")
        ));
    }
    s.push_str("\n**Mass care health tasks (CDC PHEP Capability 7)**\n");
    let tasks = content::cite_all(&c.mass_care_tasks, &c.sources);
    for t in &tasks {
        s.push_str(&content::bullet(t));
    }

    caveats.push(
        "The intake questions are this project's draft organized by the CMIST framework; FEMA's \
         Functional Needs Support Services guidance could not be retrieved to compare. Adapt the \
         form to your shelter plan and the American Red Cross or MEMA forms you use."
            .into(),
    );
    if !staffing.is_empty() {
        caveats.push(
            "Staffing ratios marked 'tool default' are placeholders with no published source; use \
             your shelter plan's ratios."
                .into(),
        );
    }
    caveats.extend(content::review_caveats(&[content::SHELTER]));
    sources.extend(content::sources_for(
        c.sources.keys().map(String::as_str),
        &c.sources,
    ));

    let any_suppressed = batch.as_ref().is_some_and(|b| {
        b.yes_counts
            .values()
            .chain(b.languages.values())
            .any(|v| v.starts_with('<'))
    });
    Ok(Envelope::new(
        s,
        ShelterResult {
            area,
            cmist_definition: c.cmist_definition.text.clone(),
            domains: c.domains.clone(),
            dispositions: c.dispositions.clone(),
            mass_care_tasks: tasks,
            electric_equipment: c.electric_equipment.items.clone(),
            expected_residents: args.expected_residents,
            needs,
            staffing,
            staffing_assumptions,
            batch,
            blank_forms,
            person_fields: PERSON_FIELDS.iter().map(|(f, _)| f.to_string()).collect(),
            translation_folder: "/data/templates/<language>/cmist_intake.md (Markdown with {{placeholders}} named like the question ids and person fields)".into(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        suppression_applied: args.intake_path.is_some(),
        suppression_threshold: args.intake_path.as_ref().map(|_| threshold),
        identifiers_detected: identifiers,
        files_written,
        notes: if any_suppressed || args.intake_path.is_some() {
            vec!["Completed forms (with names and answers) are only in the output folder; counts of 1 to the threshold are suppressed.".into()]
        } else {
            Vec::new()
        },
    })
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn agency_translations_use_the_shared_folder_names() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        // The folder name the other tools use for "Haitian Creole".
        std::fs::create_dir_all(data.join("templates/haitian_creole")).unwrap();
        std::fs::write(
            data.join("templates/haitian_creole/cmist_intake.md"),
            "# Admisyon\n\nNon: {{Name}}\n",
        )
        .unwrap();
        std::fs::write(
            data.join("intake.csv"),
            "Name,Preferred Language,interpreter_needed\n\
             Resident One,Haitian Creole,yes\n\
             Resident Two,../../outside,no\n",
        )
        .unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = functional_needs_shelter_intake(
            &ctx,
            ShelterArgs {
                area: Some("Brockton".into()),
                blank_form: Some("none".into()),
                intake_path: Some("intake.csv".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let b = env.data.batch.as_ref().unwrap();
        assert_eq!(b.forms_in_agency_translation, 1);
        // A value that is not a folder name falls back to English; it is never used as a path.
        assert_eq!(b.forms_in_english_fallback, 1);
    }

    #[tokio::test]
    async fn estimates_needs_and_fills_forms_locally() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(data.join("templates/spanish")).unwrap();
        std::fs::write(
            data.join("templates/spanish/cmist_intake.md"),
            "# Admisión\n\nNombre: {{Name}}\n\nIntérprete: {{interpreter_needed}}\n",
        )
        .unwrap();
        let mut csv = String::from(
            "Name,Preferred Language,interpreter_needed,mobility_aid,electric_equipment,Room\n",
        );
        for i in 0..12 {
            csv.push_str(&format!(
                "Resident Number{i},{},{},{},{},{}\n",
                if i < 3 {
                    "Spanish"
                } else if i < 5 {
                    "Haitian Creole"
                } else {
                    "English"
                },
                if i < 5 { "yes" } else { "no" },
                if i < 6 { "yes" } else { "no" },
                if i == 0 { "yes" } else { "no" },
                i
            ));
        }
        std::fs::write(data.join("intake.csv"), csv).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = functional_needs_shelter_intake(
            &ctx,
            ShelterArgs {
                area: Some("Brockton".into()),
                expected_residents: Some(200),
                blank_form: Some("both".into()),
                intake_path: Some("intake.csv".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let d = &env.data;
        assert_eq!(d.domains.len(), 5);
        assert_eq!(d.blank_forms.len(), 2);
        let mob = d
            .needs
            .iter()
            .find(|n| n.need.starts_with("Mobility"))
            .unwrap();
        assert!(mob.expected_residents > 0);
        assert!(d.needs.iter().any(|n| n.need.contains("electricity")));
        assert_eq!(d.staffing[0].per_shift, 4); // 200 / 50
        let b = d.batch.as_ref().unwrap();
        assert_eq!(b.registrants, 12);
        assert_eq!(b.yes_counts["mobility_aid"], "6");
        assert_eq!(b.yes_counts["interpreter_needed"], "5");
        assert_eq!(b.yes_counts["electric_equipment"], "<5");
        assert_eq!(b.languages["Spanish"], "<5");
        assert_eq!(b.forms_in_agency_translation, 3);
        assert_eq!(b.forms_in_english_fallback, 2);
        let all = format!(
            "{}{}",
            env.summary,
            serde_json::to_string(&env.data).unwrap()
        );
        assert!(!all.contains("Resident Number"));
        assert!(env.privacy.suppression_applied);
        assert!(data.join("output/cmist_shelter_intake_forms.docx").exists());
    }
}
