//! LEAD-08 `draft_standing_orders`: drafts a standing order or protocol skeleton with the sections
//! the Massachusetts Board of Registration in Nursing requires (Advisory Ruling 9324, and 9804
//! for vaccines), the nurse's own clinical text with the source she cites for it, `[NURSE: ...]`
//! and `[MEDICAL DIRECTOR: ...]` placeholders, and a Word file in /data/output for the medical
//! director's review and signature.
//!
//! No clinical content is bundled or invented: every inclusion or exclusion criterion,
//! contraindication, precaution, dose, and route in the draft is text the nurse typed with the
//! source she cites for it. The tool checks the structure, flags wording that suggests a menu of
//! choices or a change to the order (limits in Advisory Ruling 9324), and returns section status,
//! never the text itself. Decision support only: the medical director signs.

use std::collections::BTreeSet;

use chrono::{Datelike, Months, NaiveDate};
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_local::phi;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::strategy_docs::{DocBuilder, OutFormat, tidy, write_documents};
use super::workforce_content::{self as wc, SkeletonSection, StandingOrderContent};
use crate::context::Context;
use crate::md;

const MAX_SECTION_CHARS: usize = 4000;
const MAX_SECTIONS: usize = 20;

// ---------------------------------------------------------------- arguments

/// What kind of standing order this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrderKind {
    /// A vaccine standing order (adds the Board's vaccine elements: VIS, education, consent, MIIS).
    Vaccine,
    /// A standing order for another medication (for example naloxone or epinephrine).
    Medication,
    /// A standing order or protocol for a nursing activity with no medication (for example a
    /// screening or a point-of-care test).
    Activity,
}

impl OrderKind {
    fn id(self) -> &'static str {
        match self {
            OrderKind::Vaccine => "vaccine",
            OrderKind::Medication => "medication",
            OrderKind::Activity => "activity",
        }
    }
}

/// One section of the draft that you fill in.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct StandingOrderSectionArg {
    /// The section id (for example `inclusion_criteria`, `contraindications_precautions`,
    /// `dose_route`). Ask for the draft with no sections to see every id for the order type.
    pub section_id: String,
    /// The text for the section, written from your cited source. Do not type a patient's name or
    /// other identifier.
    pub text: String,
    /// Where the text comes from: the guideline, package insert, or order template, with its
    /// edition and date. Required for sections that hold clinical content (doses, criteria,
    /// contraindications, precautions, emergency steps).
    pub source_citation: Option<String>,
}

/// Arguments for `draft_standing_orders`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct StandingOrderArgs {
    /// vaccine, medication, or activity. Required.
    pub order_type: Option<OrderKind>,
    /// Title of the order, for example "Seasonal influenza vaccine for adults". Defaults to
    /// "Standing order".
    pub title: Option<String>,
    /// Name of your agency or board of health, for the heading.
    pub agency_name: Option<String>,
    /// Sections you have drafted (up to 20). Sections left out are written as placeholders.
    pub sections: Option<Vec<StandingOrderSectionArg>>,
    /// Date the order takes effect once signed (YYYY-MM-DD). The medical director confirms it.
    pub effective_date: Option<String>,
    /// Months between reviews by medical, pharmacy, and nursing staff (1 to 60). Needs
    /// effective_date. Advisory Ruling 9324 requires regular review on defined occasions but
    /// names no interval: the interval is for your medical director to set.
    pub review_interval_months: Option<u32>,
    /// Write the draft Word file to /data/output (default true).
    pub write_document: Option<bool>,
}

// ---------------------------------------------------------------- results

/// Status of one section of the draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SectionStatus {
    pub section_id: String,
    pub label: String,
    /// `nurse` or `medical_director`: who completes the section.
    pub completed_by: String,
    /// `drafted` (text supplied) or `placeholder` (still to be written).
    pub status: String,
    /// True when the section holds clinical content that must come from a cited source.
    pub clinical: bool,
    /// True when a source citation was given for the section.
    pub source_cited: bool,
    /// The Advisory Ruling 9324 element the section covers, when it covers one.
    pub ar9324_element: Option<String>,
}

/// Coverage of one of the ten required elements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ElementCoverage {
    pub number: u32,
    pub requirement: String,
    /// `drafted`, `open` (a section is still a placeholder), or `not_for_this_type`.
    pub status: String,
    pub sections: Vec<String>,
}

/// Coverage of one of the Board's vaccine elements (Advisory Ruling 9804).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VaccineElementCoverage {
    pub requirement: String,
    pub status: String,
    pub sections: Vec<String>,
}

/// A wording cue to check against the limits in Advisory Ruling 9324.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WordingFlag {
    pub section_id: String,
    /// `menu_of_choices`, `change_to_the_order`, or `prescribing`.
    pub limit: String,
    pub message: String,
}

/// A DPH standing order that may already cover the use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExistingOrder {
    pub title: String,
    pub url: String,
}

/// Review dates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OrderDates {
    pub effective_date: Option<String>,
    pub review_interval_months: Option<u32>,
    pub next_review_due: Option<String>,
    pub note: String,
}

/// Result of `draft_standing_orders`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StandingOrderResult {
    pub order_type: String,
    pub title: String,
    pub sections: Vec<SectionStatus>,
    pub drafted_sections: usize,
    pub placeholder_sections: usize,
    pub ar9324_elements: Vec<ElementCoverage>,
    pub vaccine_elements: Vec<VaccineElementCoverage>,
    pub wording_flags: Vec<WordingFlag>,
    /// DPH's signed standing orders for local boards of health (vaccine drafts only).
    pub dph_orders_to_check_first: Vec<ExistingOrder>,
    pub dates: OrderDates,
    /// Always "none": no vaccine, drug, dose, criterion, or precaution is bundled.
    pub bundled_clinical_content: String,
    pub document_file: Option<String>,
}

// ---------------------------------------------------------------- logic

fn clean_line(what: &str, s: &str, max: usize) -> Result<String> {
    let t = tidy(s);
    if t.is_empty() {
        return Err(PhnError::InvalidInput(format!("{what} is empty")));
    }
    if t.chars().count() > max {
        return Err(PhnError::InvalidInput(format!(
            "{what} is longer than {max} characters"
        )));
    }
    if !phi::scan_text(&t).is_empty() {
        return Err(PhnError::Privacy(format!(
            "{what} appears to contain an identifier; a standing order is not about one patient"
        )));
    }
    Ok(t)
}

/// Section text keeps its line breaks but is limited, trimmed, and scanned.
fn clean_block(what: &str, s: &str) -> Result<String> {
    let t = s.trim().to_string();
    if t.is_empty() {
        return Err(PhnError::InvalidInput(format!("{what} is empty")));
    }
    if t.chars().count() > MAX_SECTION_CHARS {
        return Err(PhnError::InvalidInput(format!(
            "{what} is longer than {MAX_SECTION_CHARS} characters; attach the long source text to the order instead"
        )));
    }
    if !phi::scan_text(&t).is_empty() {
        return Err(PhnError::Privacy(format!(
            "{what} appears to contain an identifier (name, address, date of birth, or record number); remove it. A standing order is not about one patient"
        )));
    }
    Ok(t)
}

fn contains_phrase(text: &str, phrase: &str) -> bool {
    let norm = |s: &str| {
        s.to_lowercase()
            .replace(['\u{2019}', '\u{2018}'], "'")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    norm(text).contains(&norm(phrase))
}

fn wording_flags(c: &StandingOrderContent, section_id: &str, text: &str) -> Vec<WordingFlag> {
    let mut out = Vec::new();
    let mut check = |phrases: &[String], limit: &str, message: &str| {
        if phrases.iter().any(|p| contains_phrase(text, p)) {
            out.push(WordingFlag {
                section_id: section_id.to_string(),
                limit: limit.to_string(),
                message: message.to_string(),
            });
        }
    };
    check(
        &c.sectional_checks.menu_phrases,
        "menu_of_choices",
        "The wording suggests the nurse chooses among medications, doses, or actions. Advisory Ruling 9324 says a standing order cannot let the nurse choose from a menu; tie each choice to a stated criterion or write a separate order.",
    );
    check(
        &c.sectional_checks.alteration_phrases,
        "change_to_the_order",
        "The wording suggests the nurse may change the order once started. Advisory Ruling 9324 says a standing order cannot let the nurse alter it or start a new medication on her own.",
    );
    check(
        &c.sectional_checks.prescribing_phrases,
        "prescribing",
        "The wording mentions prescribing, renewing, refilling, or extending. Advisory Ruling 9324 says a standing order cannot authorize a nurse to prescribe, renew, refill, or extend a prescription.",
    );
    out
}

/// Which skeleton sections cover each vaccine element (this project's mapping).
const VACCINE_MAP: [(&str, &[&str]); 9] = [
    ("v_name", &["substance_or_activity"]),
    ("v_dose_route", &["dose_route"]),
    (
        "v_criteria",
        &[
            "inclusion_criteria",
            "exclusion_criteria",
            "contraindications_precautions",
        ],
    ),
    ("v_signature", &["authorization"]),
    ("v_education", &["vis_education_consent"]),
    ("v_consent", &["vis_education_consent"]),
    ("v_miis", &["reporting"]),
    ("v_vis", &["vis_education_consent"]),
    ("v_documentation", &["documentation"]),
];

struct Drafted {
    text: String,
    source: Option<String>,
}

fn who(s: &SkeletonSection) -> &'static str {
    if s.kind == "medical_director" {
        "medical_director"
    } else {
        "nurse"
    }
}

pub fn draft_standing_orders(
    ctx: &Context,
    args: StandingOrderArgs,
) -> Result<Envelope<StandingOrderResult>> {
    let c = StandingOrderContent::load()?;
    let kind = args.order_type.ok_or_else(|| {
        PhnError::InvalidInput(
            "order_type is required: vaccine, medication, or activity (a vaccine order also needs \
             the Board's vaccine elements)"
                .into(),
        )
    })?;
    let ot = c
        .order_types
        .iter()
        .find(|t| t.id == kind.id())
        .ok_or_else(|| PhnError::Internal("order type missing".into()))?;
    let title = match &args.title {
        Some(t) if !t.trim().is_empty() => clean_line("title", t, 120)?,
        _ => "Standing order".to_string(),
    };
    let agency = match &args.agency_name {
        Some(t) if !t.trim().is_empty() => Some(clean_line("agency_name", t, 120)?),
        _ => None,
    };

    // Dates.
    let effective = match &args.effective_date {
        Some(s) if !s.trim().is_empty() => Some(super::oml::parse_date("effective_date", s)?),
        _ => None,
    };
    if let Some(m) = args.review_interval_months
        && !(1..=60).contains(&m)
    {
        return Err(PhnError::InvalidInput(
            "review_interval_months must be between 1 and 60".into(),
        ));
    }
    if args.review_interval_months.is_some() && effective.is_none() {
        return Err(PhnError::InvalidInput(
            "review_interval_months needs effective_date".into(),
        ));
    }
    let next_review: Option<NaiveDate> = match (effective, args.review_interval_months) {
        (Some(e), Some(m)) => e.checked_add_months(Months::new(m)),
        _ => None,
    };
    if let Some(n) = next_review
        && !(1900..=2100).contains(&n.year())
    {
        return Err(PhnError::InvalidInput(
            "the review date falls outside 1900 to 2100".into(),
        ));
    }

    // The nurse's sections.
    let given = args.sections.clone().unwrap_or_default();
    if given.len() > MAX_SECTIONS {
        return Err(PhnError::InvalidInput(format!(
            "at most {MAX_SECTIONS} sections"
        )));
    }
    let valid: Vec<&str> = ot.sections.iter().map(String::as_str).collect();
    let mut drafted: std::collections::BTreeMap<String, Drafted> = Default::default();
    let mut flags: Vec<WordingFlag> = Vec::new();
    for g in &given {
        let id = g.section_id.trim();
        let sec = c.section(id).filter(|_| valid.contains(&id)).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "'{id}' is not a section of this {} standing order. Sections: {}",
                kind.id(),
                valid.join(", ")
            ))
        })?;
        if drafted.contains_key(id) {
            return Err(PhnError::InvalidInput(format!(
                "section '{id}' is given twice"
            )));
        }
        if id == "authorization" {
            return Err(PhnError::InvalidInput(
                "the authorization block is completed and signed by the medical director; leave it out and the draft carries the signature lines".into(),
            ));
        }
        let text = clean_block(&format!("section '{id}'"), &g.text)?;
        let source = match g.source_citation.as_deref().map(tidy).filter(|s| !s.is_empty()) {
            Some(s) => Some(clean_line(&format!("source_citation for '{id}'"), &s, 300)?),
            None => None,
        };
        if sec.clinical && source.is_none() {
            return Err(PhnError::InvalidInput(format!(
                "section '{id}' holds clinical content, which must come from a source you cite: give source_citation (the guideline, package insert, or template, with edition and date). This tool does not supply clinical content"
            )));
        }
        flags.extend(wording_flags(&c, id, &text));
        drafted.insert(id.to_string(), Drafted { text, source });
    }

    // Section statuses in skeleton order.
    let sections: Vec<&SkeletonSection> = ot
        .sections
        .iter()
        .filter_map(|id| c.section(id))
        .collect();
    let statuses: Vec<SectionStatus> = sections
        .iter()
        .map(|s| SectionStatus {
            section_id: s.id.clone(),
            label: s.label.clone(),
            completed_by: who(s).to_string(),
            status: if drafted.contains_key(&s.id) {
                "drafted".into()
            } else {
                "placeholder".into()
            },
            clinical: s.clinical,
            source_cited: drafted.get(&s.id).is_some_and(|d| d.source.is_some()),
            ar9324_element: s.element.clone(),
        })
        .collect();
    let drafted_n = statuses.iter().filter(|s| s.status == "drafted").count();

    // Elements.
    let section_status = |id: &str| -> Option<bool> {
        statuses
            .iter()
            .find(|s| s.section_id == id)
            .map(|s| s.status == "drafted")
    };
    let elements: Vec<ElementCoverage> = c
        .required_elements
        .iter()
        .map(|e| {
            let present: Vec<&String> = e
                .section_ids
                .iter()
                .filter(|s| section_status(s).is_some())
                .collect();
            let status = if present.is_empty() {
                "not_for_this_type"
            } else if present.iter().all(|s| section_status(s) == Some(true)) {
                "drafted"
            } else {
                "open"
            };
            ElementCoverage {
                number: e.number,
                requirement: e.text.clone(),
                status: status.into(),
                sections: present.into_iter().cloned().collect(),
            }
        })
        .collect();
    // Authorization (element 4) is the medical director's signature: it is never "drafted".
    let elements: Vec<ElementCoverage> = elements
        .into_iter()
        .map(|mut e| {
            if e.sections.iter().any(|s| s == "authorization") {
                e.status = "open".into();
            }
            e
        })
        .collect();

    let vaccine_elements: Vec<VaccineElementCoverage> = if kind == OrderKind::Vaccine {
        c.vaccine_elements
            .iter()
            .map(|v| {
                let secs: Vec<String> = VACCINE_MAP
                    .iter()
                    .find(|(id, _)| *id == v.id)
                    .map(|(_, s)| s.iter().map(|x| (*x).to_string()).collect())
                    .unwrap_or_default();
                let done = !secs.is_empty()
                    && secs
                        .iter()
                        .all(|s| s != "authorization" && section_status(s) == Some(true));
                VaccineElementCoverage {
                    requirement: v.text.clone(),
                    status: if done { "drafted" } else { "open" }.into(),
                    sections: secs,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let dph: Vec<ExistingOrder> = if kind == OrderKind::Vaccine {
        c.dph_orders
            .items
            .iter()
            .map(|i| ExistingOrder {
                title: i.title.clone(),
                url: i.url.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };

    let dates = OrderDates {
        effective_date: effective.map(|d| d.to_string()),
        review_interval_months: args.review_interval_months,
        next_review_due: next_review.map(|d| d.to_string()),
        note: "Advisory Ruling 9324 requires regular review on defined occasions and authentication within a defined time frame but names no interval; the medical director sets both.".into(),
    };

    let mut result = StandingOrderResult {
        order_type: kind.id().to_string(),
        title: title.clone(),
        sections: statuses,
        drafted_sections: drafted_n,
        placeholder_sections: sections.len() - drafted_n,
        ar9324_elements: elements,
        vaccine_elements,
        wording_flags: flags,
        dph_orders_to_check_first: dph,
        dates,
        bundled_clinical_content: "none".into(),
        document_file: None,
    };

    let mut files = Vec::new();
    if args.write_document.unwrap_or(true) {
        let doc = build_document(ctx, &c, kind, &title, agency.as_deref(), &sections, &drafted, &result);
        let (_, written) =
            write_documents(ctx, "standing_orders", "standing_order_draft", &doc, &[OutFormat::Docx], None)?;
        if let Some(f) = written.into_iter().next() {
            files.push(f.clone());
            result.document_file = Some(f);
        }
    }

    let summary = summary_text(&result);
    let mut caveats = vec![
        "This tool supplies the structure only. Every dose, route, inclusion or exclusion criterion, contraindication, precaution, and emergency step in the draft is text you typed with the source you cited; nothing clinical is bundled or checked against a reference. Verify each against the current source before the medical director reviews it.".to_string(),
        "Only a duly authorized prescriber can authorize a standing order. The draft is not in effect until the medical director signs (or approves it on file with your agency), and the Board requires a way for medical, pharmacy, and nursing staff to review it on defined occasions and for the prescriber to authenticate its use within a defined time frame (Advisory Ruling 9324).".into(),
        "A standing order cannot let the nurse alter the order, choose among a menu of medications, doses, or actions, or prescribe, renew, refill, or extend a prescription (Advisory Ruling 9324). The wording check looks for word cues only; no flag does not mean the order complies.".into(),
        "Clinical decision support only: this is not legal advice and not a determination that any act is within a nurse's scope of practice. Use nursing_scope_of_practice_guide for the Board's wording and your agency's counsel or the Board for questions.".into(),
    ];
    if kind == OrderKind::Vaccine {
        caveats.push("DPH issues signed standing orders for local boards of health (influenza, COVID-19, and anaphylaxis medication, 2026-2027); a board of health that gives those vaccines should start from DPH's order for the season rather than draft its own. Immunize.org's template asks that Immunize.org be acknowledged as its source if you adapt it.".into());
    }
    caveats.extend(wc::caveats(&[wc::STANDING_ORDERS]));
    let mut sources = wc::cite(&["ar9324"], &c.sources);
    if kind == OrderKind::Vaccine {
        sources.extend(wc::cite(
            &["ar9804", "dph_hub", "dph_lboh_flu", "immunize_template", "immunize_hub"],
            &c.sources,
        ));
    }
    sources.push(Source::computed(
        "Standing order section and element checklist",
        "Calculated by this server from the nurse's inputs",
    ));
    let privacy = Privacy {
        row_level_data: false,
        suppression_applied: false,
        suppression_threshold: None,
        identifiers_detected: Vec::new(),
        files_written: files,
        notes: vec![
            "The section text you typed is written only to the draft file in the output folder; it is not returned.".into(),
            "No patient data is read or accepted: text that looks like an identifier is refused.".into(),
        ],
    };
    Ok(Envelope::new(summary, result)
        .decision_support()
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

fn summary_text(r: &StandingOrderResult) -> String {
    let mut s = format!(
        "**Standing order draft ({}): {}.** {} of {} sections drafted by you; {} are placeholders for you or the medical director.\n\n",
        r.order_type,
        r.title,
        r.drafted_sections,
        r.sections.len(),
        r.placeholder_sections
    );
    let rows: Vec<Vec<String>> = r
        .sections
        .iter()
        .map(|x| {
            vec![
                x.label.clone(),
                if x.completed_by == "nurse" {
                    "nurse".into()
                } else {
                    "medical director".into()
                },
                x.status.clone(),
                if x.clinical {
                    if x.source_cited { "cited" } else { "needed" }.into()
                } else {
                    "-".into()
                },
            ]
        })
        .collect();
    s.push_str(&md::table(&["Section", "Completed by", "Status", "Source"], &rows));
    s.push_str("\n**Board of Registration in Nursing required elements (Advisory Ruling 9324)**\n\n");
    let rows: Vec<Vec<String>> = r
        .ar9324_elements
        .iter()
        .map(|e| vec![e.number.to_string(), e.requirement.clone(), e.status.clone()])
        .collect();
    s.push_str(&md::table(&["#", "Element", "Status"], &rows));
    if !r.vaccine_elements.is_empty() {
        s.push_str("\n**Vaccine elements (Advisory Ruling 9804)**\n\n");
        let rows: Vec<Vec<String>> = r
            .vaccine_elements
            .iter()
            .map(|e| vec![e.requirement.clone(), e.status.clone()])
            .collect();
        s.push_str(&md::table(&["Element", "Status"], &rows));
    }
    if !r.wording_flags.is_empty() {
        s.push_str("\n**Wording to re-read against the limits in Advisory Ruling 9324**\n\n");
        for f in &r.wording_flags {
            s.push_str(&format!("- Section `{}`: {}\n", f.section_id, f.message));
        }
    }
    if !r.dph_orders_to_check_first.is_empty() {
        s.push_str("\n**Check first:** DPH has signed standing orders for local boards of health for 2026-2027 (influenza, COVID-19, and anaphylaxis medication). Use DPH's order for those vaccines if it fits; the links are in the result.\n");
    }
    if let Some(d) = &r.dates.next_review_due {
        s.push_str(&format!("\nNext review due {d} (your interval; the medical director confirms).\n"));
    }
    if let Some(f) = &r.document_file {
        s.push_str(&format!("\nDraft for the medical director: {f}\n"));
    }
    s
}

#[allow(clippy::too_many_arguments)]
fn build_document(
    ctx: &Context,
    c: &StandingOrderContent,
    kind: OrderKind,
    title: &str,
    agency: Option<&str>,
    sections: &[&SkeletonSection],
    drafted: &std::collections::BTreeMap<String, Drafted>,
    r: &StandingOrderResult,
) -> phn_docs::model::Document {
    let mut b = DocBuilder::new();
    b.para("DRAFT for medical director review. Not in effect until signed by a duly authorized prescriber (or approved and kept on file with the agency). Placeholders in square brackets are to be completed by the person named.");
    if let Some(a) = agency {
        b.labeled("Agency:", a);
    }
    b.labeled(
        "Type:",
        match kind {
            OrderKind::Vaccine => "Vaccine administration",
            OrderKind::Medication => "Medication",
            OrderKind::Activity => "Nursing activity",
        },
    );
    b.labeled(
        "Effective date:",
        r.dates
            .effective_date
            .clone()
            .unwrap_or_else(|| "[MEDICAL DIRECTOR: effective date]".into()),
    );
    b.labeled(
        "Next review due:",
        r.dates
            .next_review_due
            .clone()
            .unwrap_or_else(|| "[MEDICAL DIRECTOR: review date]".into()),
    );
    if kind == OrderKind::Vaccine {
        b.para("Before using this draft, check DPH's Vaccine Model Standing Orders page (mass.gov/lists/vaccine-model-standing-orders) for a signed standing order for local boards of health that covers the vaccine. If this draft adapts an Immunize.org template, acknowledge Immunize.org as its source.");
    }
    for (i, s) in sections
        .iter()
        .filter(|s| s.id != "authorization")
        .enumerate()
    {
        b.heading(2, format!("{}. {}", i + 1, s.label));
        match drafted.get(&s.id) {
            Some(d) => {
                for line in d.text.lines().filter(|l| !l.trim().is_empty()) {
                    b.para(line.trim());
                }
                if let Some(src) = &d.source {
                    b.labeled("Source cited by the nurse:", src.clone());
                }
            }
            None => {
                b.para(s.placeholder.clone());
                b.para(format!("Guidance: {}", s.guidance));
            }
        }
    }
    b.heading(2, "Authorization");
    b.para("This policy and procedure remains in effect for all patients of the agency named above from the effective date until rescinded or until the review date, whichever is first.");
    b.table(
        "Authorization",
        &["Item", "Entry"],
        vec![
            vec!["Medical director (print name)".into(), "[MEDICAL DIRECTOR: name]".into()],
            vec!["Signature".into(), "[MEDICAL DIRECTOR: signature]".into()],
            vec!["Date".into(), "[MEDICAL DIRECTOR: date]".into()],
            vec!["Reviewed by nursing (print name and title)".into(), "[NURSE: name and title]".into()],
            vec!["Reviewed by pharmacy, if required by the agency".into(), "[PHARMACY: name and date]".into()],
        ],
    );
    b.heading(2, "Checklist against Board of Registration in Nursing Advisory Ruling 9324");
    b.table(
        "AR 9324 checklist",
        &["#", "Required element", "Status in this draft"],
        r.ar9324_elements
            .iter()
            .map(|e| vec![e.number.to_string(), e.requirement.clone(), e.status.clone()])
            .collect(),
    );
    let limits: Vec<String> = c.limits.iter().map(|l| l.text.clone()).collect();
    b.para("A standing order cannot authorize the nurse to:");
    b.list(&limits);
    if !r.wording_flags.is_empty() {
        b.heading(3, "Wording to re-read");
        let msgs: Vec<String> = r
            .wording_flags
            .iter()
            .map(|f| format!("Section {}: {}", f.section_id, f.message))
            .collect();
        b.list(&msgs);
    }
    if !r.vaccine_elements.is_empty() {
        b.heading(3, "Vaccine elements (Advisory Ruling 9804)");
        b.table(
            "AR 9804 elements",
            &["Element", "Status in this draft"],
            r.vaccine_elements
                .iter()
                .map(|e| vec![e.requirement.clone(), e.status.clone()])
                .collect(),
        );
    }
    b.para("Sources for the structure: Massachusetts Board of Registration in Nursing Advisory Ruling 9324 (revised April 11, 2018)");
    ctx_finish(ctx, b, title)
}

fn ctx_finish(ctx: &Context, b: DocBuilder, title: &str) -> phn_docs::model::Document {
    b.finish(ctx, title, Some("Standing order / protocol (draft)".into()))
}

/// Section ids valid for an order type (used by tests).
#[allow(dead_code)]
pub(crate) fn section_ids(kind: OrderKind) -> Result<BTreeSet<String>> {
    let c = StandingOrderContent::load()?;
    Ok(c.order_types
        .iter()
        .find(|t| t.id == kind.id())
        .map(|t| t.sections.iter().cloned().collect())
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_wording_is_flagged_and_plain_wording_is_not() {
        let c = StandingOrderContent::load().unwrap();
        assert!(!wording_flags(&c, "x", "The nurse may choose between products.").is_empty());
        assert!(!wording_flags(&c, "x", "At the nurse\u{2019}s discretion, give a second dose.").is_empty());
        assert!(!wording_flags(&c, "x", "Do not refill or renew.").is_empty());
        assert!(wording_flags(&c, "x", "Give the dose in the table for the patient's age band.").is_empty());
    }

    #[test]
    fn every_order_type_has_the_required_elements_and_an_authorization_section() {
        for k in [OrderKind::Vaccine, OrderKind::Medication, OrderKind::Activity] {
            let ids = section_ids(k).unwrap();
            assert!(ids.contains("authorization"), "{k:?}");
            assert!(ids.contains("review_mechanism"));
            assert!(ids.contains("authentication"));
            assert!(ids.contains("inclusion_criteria"));
            assert!(ids.contains("emergency"));
        }
        assert!(section_ids(OrderKind::Vaccine).unwrap().contains("vis_education_consent"));
        assert!(!section_ids(OrderKind::Activity).unwrap().contains("dose_route"));
    }
}
