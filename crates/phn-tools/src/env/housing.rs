//! ENV-07 `housing_conditions_health_guide`: housing conditions that affect
//! health under the State Sanitary Code (105 CMR 410.000) — the code
//! sections, owner and occupant responsibilities, health effects and who is
//! most at risk, how to get a board of health inspection and what happens
//! next, and tenant protections (decision support, not legal advice). Writes
//! plain-language tenant teaching sheets (checked with the COMM-01
//! readability engine; agency-approved translations from
//! /data/templates/<language>/ only) and, from a local household file,
//! inspection and repair request letters filled into /data/output only
//! (template + local fill, ADR 0008).

use std::collections::{BTreeMap, BTreeSet};

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::comms::readability::{AnalyzeOptions, analyze, default_target_grade};
use crate::context::Context;
use crate::epr::area::{self, Area, AreaLevel};
use crate::epr::content::{self, Cited, CitedOut, DocSource};
use crate::md;
use crate::school::templates::{self, DocFormat, LanguageCount, TemplateResolver};

/// Content id of `content/env/housing_conditions.yaml`.
pub const HOUSING_CONDITIONS: &str = "env-housing-conditions";

/// Blank written into a sheet or letter where a value is not known.
const BLANK: &str = "____________";

// ---------------------------------------------------------------- content

/// A code section and what it requires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CodeItem {
    /// Section of 105 CMR 410.000 (or the statute) that says it.
    pub section: String,
    pub text: String,
}

/// One housing condition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionDef {
    pub id: String,
    pub label: String,
    pub keywords: Vec<String>,
    pub urgent: bool,
    pub urgent_note: String,
    pub code: Vec<CodeItem>,
    pub owner: Vec<String>,
    pub occupant: Vec<String>,
    pub health_effects: Vec<Cited>,
    pub at_risk: Vec<String>,
    pub advice: Vec<Cited>,
}

/// The 410.630(A) list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UrgentList {
    pub source: String,
    pub section: String,
    pub items: Vec<String>,
}

/// Inspection steps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectionDef {
    pub request: Vec<Cited>,
    pub during: Vec<Cited>,
    pub after: Vec<Cited>,
    pub hearings: Vec<Cited>,
}

/// A tenant protection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtectionDef {
    pub id: String,
    pub label: String,
    pub text: String,
    pub source: String,
    pub section: String,
    pub confidence: content::Confidence,
}

/// A tenant teaching sheet template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetDef {
    pub id: String,
    pub title: String,
    pub condition: Option<String>,
    pub sources: Vec<String>,
    pub markdown: String,
}

/// A letter template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LetterDef {
    pub kind: String,
    pub description: String,
    pub markdown: String,
}

/// Letter templates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LettersDef {
    pub placeholders: Vec<String>,
    pub inspection_request: LetterDef,
    pub repair_request: LetterDef,
}

/// `content/env/housing_conditions.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HousingContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub conditions: Vec<ConditionDef>,
    pub urgent_list: UrgentList,
    pub inspection: InspectionDef,
    pub tenant_protections: Vec<ProtectionDef>,
    pub legal_notice: String,
    pub teaching_sheets: Vec<SheetDef>,
    pub letters: LettersDef,
}

// ---------------------------------------------------------------- arguments and results

/// Arguments for `housing_conditions_health_guide`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct HousingGuideArgs {
    /// Conditions to cover, by name or in the tenant's words: "mold", "pests", "roaches and a
    /// leak", "no heat", "hot water", "lead paint", "smoke or carbon monoxide alarms", "trash".
    /// Default: all.
    pub conditions: Vec<String>,
    /// A Massachusetts city or town, or a county, for local housing facts (age of housing,
    /// renters). Default: your saved town (no local facts when none is saved).
    pub area: Option<String>,
    /// Write plain-language tenant teaching sheets for the conditions (and how to get an
    /// inspection) to /data/output (default true).
    pub teaching_sheets: Option<bool>,
    /// Document format for sheets and letters: "docx" (default) or "pdf".
    pub format: Option<DocFormat>,
    /// Languages for the teaching sheets besides English (e.g. ["Spanish", "Portuguese"]). Only
    /// your agency's approved translations in /data/templates/<language>/housing_<sheet>.md are
    /// used; the server never machine-translates.
    pub languages: Vec<String>,
    /// A CSV/XLSX file in /data with one row per household (columns such as name, address,
    /// unit, town, phone, owner, problems, date noticed, language). The inspection request and
    /// repair request letters are filled from it into /data/output only; nothing from the file
    /// is returned.
    pub households_path: Option<String>,
    /// Letters to fill from households_path: "inspection_request", "repair_request", or "both"
    /// (default both).
    pub letters: Option<String>,
}

/// One condition's guide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConditionGuide {
    pub id: String,
    pub label: String,
    /// True when 105 CMR 410.630(A) always deems it a condition that may endanger health
    /// (inspection within 1 business day; good faith correction within 24 hours).
    pub urgent: bool,
    pub urgent_note: String,
    /// What the code requires, by section.
    pub code: Vec<CodeItem>,
    /// What the owner must do.
    pub owner: Vec<String>,
    /// What the occupant must do.
    pub occupant: Vec<String>,
    pub health_effects: Vec<CitedOut>,
    pub at_risk: Vec<String>,
    /// Practical advice (CDC, EPA, DPH).
    pub advice: Vec<CitedOut>,
}

/// The inspection process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InspectionGuide {
    pub request: Vec<CitedOut>,
    pub during: Vec<CitedOut>,
    pub after: Vec<CitedOut>,
    pub hearings: Vec<CitedOut>,
    /// Conditions 410.630(A) always deems may endanger health.
    pub urgent_conditions: Vec<String>,
}

/// Who to call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BoardOfHealth {
    pub town: Option<String>,
    /// The board of health phone saved with configure_jurisdiction, if any.
    pub phone: Option<String>,
    pub how_to_find: String,
}

/// A tenant protection (decision support, not legal advice).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProtectionOut {
    pub id: String,
    pub label: String,
    pub text: String,
    pub citation: String,
}

/// Local housing facts (ACS).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LocalHousing {
    pub area: String,
    /// Percent of housing units built before 1980 (lead paint was banned for housing in 1978;
    /// ACS groups years by decade).
    pub built_before_1980_pct: Option<f64>,
    pub built_before_1950_pct: Option<f64>,
    pub housing_units: Option<f64>,
    /// Percent of occupied housing units that are renter-occupied.
    pub renter_pct: Option<f64>,
    /// Percent of renter households paying 30% or more of income for housing.
    pub renter_cost_burden_pct: Option<f64>,
    /// Percent of occupied units with more than one person per room.
    pub crowding_pct: Option<f64>,
    pub source: String,
}

/// A translation of a teaching sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SheetTranslation {
    pub language: String,
    /// The file written from the agency's translation (none when there is no translation).
    pub file: Option<String>,
    /// "agency translation" or "no agency translation found: English only".
    pub status: String,
}

/// One teaching sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SheetOut {
    pub id: String,
    pub title: String,
    /// Template kind for agency translations: /data/templates/<language>/<kind>.md.
    pub template_kind: String,
    pub file: Option<String>,
    /// Median of the Flesch-Kincaid, SMOG, and Fog grades (COMM-01 engine).
    pub reading_grade: f64,
    pub target_grade: f64,
    pub meets_target: bool,
    pub translations: Vec<SheetTranslation>,
}

/// A letter template the nurse can review (filled only from a local file).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LetterTemplateOut {
    pub kind: String,
    pub description: String,
    pub placeholders: Vec<String>,
    pub markdown: String,
}

/// Letters filled from a local household file (counts only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LettersWritten {
    pub source_file: String,
    pub folder: String,
    pub households: usize,
    pub letters: usize,
    pub files: Vec<String>,
    pub by_language: Vec<LanguageCount>,
    /// Letters that used English because no agency translation was found.
    pub english_fallback: usize,
    /// Letter fields with no value (written as a blank line to fill in by hand).
    pub blank_fields: usize,
    /// Template fields the file supplied.
    pub fields_found: Vec<String>,
    pub unknown_placeholders: Vec<String>,
}

/// Result of `housing_conditions_health_guide`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HousingGuideResult {
    /// The regulation and version used.
    pub regulation: String,
    pub conditions: Vec<ConditionGuide>,
    /// Words that matched no condition.
    pub unmatched_terms: Vec<String>,
    pub inspection: InspectionGuide,
    pub board_of_health: BoardOfHealth,
    pub tenant_protections: Vec<ProtectionOut>,
    pub legal_notice: String,
    pub local_housing: Option<LocalHousing>,
    pub teaching_sheets: Vec<SheetOut>,
    pub teaching_sheet_folder: Option<String>,
    pub letter_templates: Vec<LetterTemplateOut>,
    pub letters: Option<LettersWritten>,
    pub how_to_add_translations: String,
}

// ---------------------------------------------------------------- helpers

fn words(s: &str) -> String {
    format!(" {} ", phn_core::text::normalize(&s.replace('_', " ")))
}

/// Conditions matching the requested terms (all when none), and terms that
/// matched nothing. Keywords match on word boundaries.
fn match_conditions<'a>(
    c: &'a HousingContent,
    terms: &[String],
) -> (Vec<&'a ConditionDef>, Vec<String>) {
    let terms: Vec<&String> = terms.iter().filter(|t| !t.trim().is_empty()).collect();
    if terms.is_empty() || terms.iter().any(|t| words(t).trim() == "all") {
        return (c.conditions.iter().collect(), Vec::new());
    }
    let mut picked: BTreeSet<usize> = BTreeSet::new();
    let mut unmatched = Vec::new();
    for t in terms {
        let w = words(t);
        let mut hit = false;
        for (i, cd) in c.conditions.iter().enumerate() {
            let m = w.trim() == words(&cd.id).trim()
                || w.trim() == words(&cd.label).trim()
                || cd.keywords.iter().any(|k| w.contains(&words(k)));
            if m {
                picked.insert(i);
                hit = true;
            }
        }
        if !hit {
            unmatched.push(t.trim().to_string());
        }
    }
    (
        picked.into_iter().map(|i| &c.conditions[i]).collect(),
        unmatched,
    )
}

fn parse_pct(v: &str) -> Option<f64> {
    v.trim().parse::<f64>().ok()
}

/// ACS housing facts for an area.
fn local_housing(ctx: &Context, a: &Area) -> Result<(Option<LocalHousing>, Vec<Source>)> {
    let (hous, hous_entry, _) = ctx
        .snapshots
        .file_text("ma-acs5-housing-age", "ma_acs5_housing_age.csv")?;
    let (ind, ind_entry, _) = ctx
        .snapshots
        .file_text("census-acs5-ma", "ma_acs5_indicators.csv")?;
    let row = |text: &str, geoid: &str| -> Result<Option<BTreeMap<String, String>>> {
        let mut rdr = csv::Reader::from_reader(text.as_bytes());
        let h: Vec<String> = rdr
            .headers()
            .map_err(|e| PhnError::Internal(format!("ACS snapshot: {e}")))?
            .iter()
            .map(str::to_string)
            .collect();
        for r in rdr.records() {
            let r = r.map_err(|e| PhnError::Internal(format!("ACS snapshot: {e}")))?;
            if r.get(0) == Some(geoid) {
                return Ok(Some(
                    h.iter()
                        .cloned()
                        .zip(r.iter().map(str::to_string))
                        .collect(),
                ));
            }
        }
        Ok(None)
    };
    let h = row(&hous, &a.geoid)?;
    let i = row(&ind, &a.geoid)?;
    if h.is_none() && i.is_none() {
        return Ok((None, Vec::new()));
    }
    let get = |m: &Option<BTreeMap<String, String>>, k: &str| {
        m.as_ref().and_then(|m| m.get(k)).and_then(|v| parse_pct(v))
    };
    Ok((
        Some(LocalHousing {
            area: a.name.clone(),
            built_before_1980_pct: get(&h, "pre1980_pct"),
            built_before_1950_pct: get(&h, "pre1950_pct"),
            housing_units: get(&h, "housing_units"),
            renter_pct: get(&i, "renter_pct"),
            renter_cost_burden_pct: get(&i, "renter_cost_burden_pct"),
            crowding_pct: get(&i, "crowding_pct"),
            source: format!(
                "{} ({}); {} ({})",
                hous_entry.title,
                hous_entry.source_vintage,
                ind_entry.title,
                ind_entry.source_vintage
            ),
        }),
        vec![hous_entry.to_source(), ind_entry.to_source()],
    ))
}

/// Household-file columns for each letter field.
const HOUSEHOLD_FIELDS: &[(&str, &[&str])] = &[
    (
        "tenant_name",
        &[
            "name",
            "tenant name",
            "tenant",
            "occupant",
            "occupant name",
            "full name",
            "resident",
        ],
    ),
    (
        "address",
        &["address", "street address", "street", "address line 1"],
    ),
    (
        "unit",
        &["unit", "apartment", "apt", "unit number", "apt number"],
    ),
    ("town", &["town", "city", "city or town", "municipality"]),
    (
        "phone",
        &["phone", "telephone", "phone number", "cell", "mobile"],
    ),
    (
        "owner_name",
        &[
            "owner",
            "owner name",
            "landlord",
            "landlord name",
            "property manager",
            "manager",
        ],
    ),
    (
        "problems",
        &[
            "problems",
            "problem",
            "conditions",
            "condition",
            "issues",
            "complaint",
            "description",
        ],
    ),
    (
        "date_first_noticed",
        &[
            "date noticed",
            "date first noticed",
            "noticed",
            "first noticed",
            "since",
        ],
    ),
];
const LANGUAGE_COLUMNS: &[&str] = &[
    "language",
    "preferred language",
    "primary language",
    "home language",
];

fn find_column(headers: &[String], aliases: &[&str]) -> Option<usize> {
    let norm: Vec<String> = headers
        .iter()
        .map(|h| phn_core::text::normalize(h))
        .collect();
    aliases
        .iter()
        .find_map(|a| norm.iter().position(|h| *h == phn_core::text::normalize(a)))
}

fn letter_kinds<'a>(c: &'a HousingContent, which: Option<&str>) -> Result<Vec<&'a LetterDef>> {
    let w = which
        .map(|s| words(s).trim().to_string())
        .unwrap_or_default();
    match w.as_str() {
        "" | "both" | "all" => Ok(vec![
            &c.letters.inspection_request,
            &c.letters.repair_request,
        ]),
        "inspection request" | "inspection" | "housing inspection request" => {
            Ok(vec![&c.letters.inspection_request])
        }
        "repair request" | "repair" | "repairs" | "housing repair request" => {
            Ok(vec![&c.letters.repair_request])
        }
        _ => Err(PhnError::InvalidInput(
            "letters must be inspection_request, repair_request, or both".into(),
        )),
    }
}

fn sheet_kind(id: &str) -> String {
    format!("housing_{id}")
}

// ---------------------------------------------------------------- the tool

/// Housing conditions and health guide.
pub fn housing_conditions_health_guide(
    ctx: &Context,
    args: HousingGuideArgs,
) -> Result<Envelope<HousingGuideResult>> {
    let c: HousingContent = content::load(HOUSING_CONDITIONS)?;
    let (picked, unmatched) = match_conditions(&c, &args.conditions);
    if picked.is_empty() {
        let names: Vec<&str> = c.conditions.iter().map(|x| x.id.as_str()).collect();
        return Err(PhnError::InvalidInput(format!(
            "no housing condition matches {}. Conditions covered: {}",
            unmatched.join(", "),
            names.join(", ")
        )));
    }
    let letter_defs = letter_kinds(&c, args.letters.as_deref())?;
    if args.letters.is_some() && args.households_path.is_none() {
        return Err(PhnError::InvalidInput(
            "letters are filled from a household file: give households_path (a CSV/XLSX in /data)"
                .into(),
        ));
    }
    let settings = ctx.settings();
    let area: Option<Area> = match args.area.as_deref() {
        Some(q) if !q.trim().is_empty() => Some(area::resolve(ctx, Some(q), true)?),
        _ => settings
            .jurisdiction
            .town
            .as_deref()
            .and_then(|t| area::resolve(ctx, Some(t), true).ok()),
    };
    let mut caveats: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    let format = args.format.unwrap_or_default();

    // Board of health contact (saved settings only).
    let saved_town = settings.jurisdiction.town.clone();
    let boh_phone = settings
        .jurisdiction
        .board_of_health_phone
        .clone()
        .filter(|p| !p.trim().is_empty());
    let board_of_health = BoardOfHealth {
        town: saved_town.clone(),
        phone: boh_phone.clone(),
        how_to_find: "Contact the local board of health (health department) through the city or \
                      town hall or its website; city and town websites are listed at \
                      mass.gov/lists/massachusetts-city-and-town-websites (DPH Notice of \
                      Occupants' Legal Rights and Responsibilities). Save your board of health \
                      phone with configure_jurisdiction to print it on the teaching sheets."
            .into(),
    };

    // Conditions.
    let conditions: Vec<ConditionGuide> = picked
        .iter()
        .map(|d| ConditionGuide {
            id: d.id.clone(),
            label: d.label.clone(),
            urgent: d.urgent,
            urgent_note: d.urgent_note.trim().to_string(),
            code: d.code.clone(),
            owner: d.owner.clone(),
            occupant: d.occupant.clone(),
            health_effects: content::cite_all(&d.health_effects, &c.sources),
            at_risk: d.at_risk.clone(),
            advice: content::cite_all(&d.advice, &c.sources),
        })
        .collect();
    if !unmatched.is_empty() {
        caveats.push(format!(
            "No condition matched: {}. Conditions covered: {}.",
            unmatched.join(", "),
            c.conditions
                .iter()
                .map(|x| x.label.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let inspection = InspectionGuide {
        request: content::cite_all(&c.inspection.request, &c.sources),
        during: content::cite_all(&c.inspection.during, &c.sources),
        after: content::cite_all(&c.inspection.after, &c.sources),
        hearings: content::cite_all(&c.inspection.hearings, &c.sources),
        urgent_conditions: c.urgent_list.items.clone(),
    };
    let title = |k: &str| {
        c.sources
            .get(k)
            .map(|s| s.title.clone())
            .unwrap_or_else(|| k.to_string())
    };
    let tenant_protections: Vec<ProtectionOut> = c
        .tenant_protections
        .iter()
        .map(|p| ProtectionOut {
            id: p.id.clone(),
            label: p.label.clone(),
            text: p.text.split_whitespace().collect::<Vec<_>>().join(" "),
            citation: format!("{}, {}", title(&p.source), p.section),
        })
        .collect();

    // Local housing facts.
    let mut local = None;
    if let Some(a) = &area {
        if a.level == AreaLevel::State {
            caveats.push("Local housing facts are given for a town or county.".into());
        }
        let (l, s) = local_housing(ctx, a)?;
        local = l;
        sources.extend(s);
    }

    // Teaching sheets.
    let target = default_target_grade();
    let mut values: BTreeMap<String, String> = BTreeMap::new();
    values.insert(
        "board_of_health_phone".into(),
        boh_phone.clone().unwrap_or_else(|| BLANK.into()),
    );
    values.insert(
        "town".into(),
        saved_town.clone().unwrap_or_else(|| BLANK.into()),
    );
    let picked_ids: BTreeSet<&str> = picked.iter().map(|d| d.id.as_str()).collect();
    let sheets: Vec<&SheetDef> = c
        .teaching_sheets
        .iter()
        .filter(|s| match &s.condition {
            None => true,
            Some(id) => picked_ids.contains(id.as_str()),
        })
        .collect();
    let languages: Vec<String> = {
        let mut v: Vec<String> = Vec::new();
        for l in &args.languages {
            if l.trim().is_empty() || templates::is_english(l) {
                continue;
            }
            let label = templates::language_label(l);
            if !v.contains(&label) {
                v.push(label);
            }
        }
        v
    };
    let write_sheets = args.teaching_sheets.unwrap_or(true);
    if !write_sheets && !languages.is_empty() {
        caveats.push("languages applies only when teaching sheets are written.".into());
    }
    let mut files_written: Vec<String> = Vec::new();
    let mut sheet_out = Vec::new();
    let mut sheet_folder = None;
    let mut folder: Option<std::path::PathBuf> = None;
    if write_sheets {
        let (dir, shown) = templates::output_folder(ctx, "housing_teaching_sheets")?;
        folder = Some(dir);
        sheet_folder = Some(shown);
    }
    for s in &sheets {
        let kind = sheet_kind(&s.id);
        let (filled, _) = templates::fill(&s.markdown, &values, BLANK);
        let a = analyze(
            &filled,
            &AnalyzeOptions {
                target_grade: target,
                max_flags: 3,
                redact_excerpts: false,
            },
        );
        let mut out = SheetOut {
            id: s.id.clone(),
            title: s.title.clone(),
            template_kind: kind.clone(),
            file: None,
            reading_grade: a.grade.median_grade,
            target_grade: target,
            meets_target: a.grade.median_grade <= target,
            translations: Vec::new(),
        };
        if let Some(dir) = &folder {
            let doc = templates::document(ctx, &filled, "en-US", &s.title);
            let f = templates::write_in(
                ctx,
                dir,
                &format!("{kind}_english.{}", format.ext()),
                &format.render(&doc)?,
            )?;
            files_written.push(f.clone());
            out.file = Some(f);
            if !languages.is_empty() {
                files_written.push(templates::write_in(
                    ctx,
                    dir,
                    &format!("template_english_{kind}.md"),
                    s.markdown.as_bytes(),
                )?);
            }
            let mut resolver = TemplateResolver::new(ctx, &kind, s.markdown.clone());
            for lang in &languages {
                let chosen = resolver.choose(lang);
                if chosen.translated {
                    let (filled, _) = templates::fill(&chosen.markdown, &values, BLANK);
                    let doc = templates::document(ctx, &filled, &chosen.tag, &s.title);
                    let f = templates::write_in(
                        ctx,
                        dir,
                        &format!(
                            "{kind}_{}.{}",
                            chosen.language.to_lowercase().replace(' ', "_"),
                            format.ext()
                        ),
                        &format.render(&doc)?,
                    )?;
                    files_written.push(f.clone());
                    out.translations.push(SheetTranslation {
                        language: chosen.language.clone(),
                        file: Some(f),
                        status: "agency translation".into(),
                    });
                } else {
                    out.translations.push(SheetTranslation {
                        language: chosen.language.clone(),
                        file: None,
                        status: "no agency translation found: English only".into(),
                    });
                }
            }
        }
        sheet_out.push(out);
    }
    for s in sheet_out.iter().filter(|s| !s.meets_target) {
        caveats.push(format!(
            "Teaching sheet '{}' reads at grade {:.1}, above the target of {target}; simplify it \
             before use (check_readability shows which sentences).",
            s.title, s.reading_grade
        ));
    }
    if write_sheets && boh_phone.is_none() {
        caveats.push(
            "No board of health phone is saved, so the sheets have a blank line for it; save it \
             with configure_jurisdiction to fill it in."
                .into(),
        );
    }
    let missing_translations: BTreeSet<String> = sheet_out
        .iter()
        .flat_map(|s| s.translations.iter())
        .filter(|t| t.file.is_none())
        .map(|t| t.language.clone())
        .collect();
    if !missing_translations.is_empty() {
        caveats.push(format!(
            "No agency-approved translation was found for: {}. Only the English sheets were \
             written for those languages; this server does not machine-translate.",
            missing_translations
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    // Letter templates and local fill.
    let letter_templates: Vec<LetterTemplateOut> = letter_defs
        .iter()
        .map(|l| LetterTemplateOut {
            kind: l.kind.clone(),
            description: l.description.clone(),
            placeholders: c.letters.placeholders.clone(),
            markdown: l.markdown.clone(),
        })
        .collect();
    let mut identifiers = Vec::new();
    let mut letters = None;
    let mut local_source = None;
    if let Some(p) = &args.households_path {
        let lt = crate::localdata::load(ctx, p, None)?;
        identifiers = lt.identifier_categories();
        let headers = &lt.table.headers;
        let cols: Vec<(&str, Option<usize>)> = HOUSEHOLD_FIELDS
            .iter()
            .map(|(f, al)| (*f, find_column(headers, al)))
            .collect();
        let found: Vec<String> = cols
            .iter()
            .filter(|(_, i)| i.is_some())
            .map(|(f, _)| (*f).to_string())
            .collect();
        if !found.iter().any(|f| f == "address" || f == "tenant_name") {
            return Err(PhnError::InvalidInput(format!(
                "'{}' needs a name or address column for the letters (for example name, \
                 address, unit, town, phone, owner, problems, date noticed, language); it has {} \
                 column(s) with other names",
                lt.label,
                headers.len()
            )));
        }
        let lang_col = find_column(headers, LANGUAGE_COLUMNS);
        let (dir, shown) = templates::output_folder(ctx, "housing_letters")?;
        let today = chrono::Local::now().date_naive().to_string();
        let mut households = 0usize;
        let mut blank_fields = 0usize;
        let mut fallback = 0usize;
        let mut unknown: BTreeSet<String> = BTreeSet::new();
        let mut files = Vec::new();
        // (letter kind, language label) -> (tag, translated, documents)
        let mut groups: BTreeMap<(String, String), (String, bool, Vec<phn_docs::model::Document>)> =
            BTreeMap::new();
        let mut resolvers: Vec<TemplateResolver> = letter_defs
            .iter()
            .map(|l| TemplateResolver::new(ctx, &l.kind, l.markdown.clone()))
            .collect();
        for row in &lt.table.rows {
            if row.iter().all(|v| v.trim().is_empty()) {
                continue;
            }
            households += 1;
            let mut v: BTreeMap<String, String> = BTreeMap::new();
            for (f, i) in &cols {
                let val = i
                    .and_then(|i| row.get(i))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                if val.is_empty() {
                    blank_fields += 1;
                }
                v.insert((*f).to_string(), val);
            }
            v.insert("date".into(), today.clone());
            let lang_raw = lang_col
                .and_then(|i| row.get(i))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            let lang_label = crate::school::screenings::result_language_label(&lang_raw);
            for (k, l) in letter_defs.iter().enumerate() {
                let chosen = resolvers[k].choose(&lang_raw);
                if !chosen.translated && !templates::is_english(&lang_raw) {
                    fallback += 1;
                }
                let (filled, unk) = templates::fill(&chosen.markdown, &v, BLANK);
                unknown.extend(unk);
                let d = templates::document(ctx, &filled, &chosen.tag, &l.kind);
                groups
                    .entry((l.kind.clone(), lang_label.clone()))
                    .or_insert_with(|| (chosen.tag.clone(), chosen.translated, Vec::new()))
                    .2
                    .push(d);
            }
        }
        if households == 0 {
            return Err(PhnError::InvalidInput(format!(
                "'{}' has no household rows",
                lt.label
            )));
        }
        let mut by_lang: BTreeMap<String, (usize, bool)> = BTreeMap::new();
        let mut n_letters = 0usize;
        for ((kind, lang), (tag, translated, docs)) in groups {
            n_letters += docs.len();
            let e = by_lang.entry(lang.clone()).or_insert((0, translated));
            e.0 += docs.len();
            let title = if kind.ends_with("inspection_request") {
                "Housing inspection requests"
            } else {
                "Housing repair requests"
            };
            let combined = templates::combine(title, &tag, docs, ctx);
            files.push(templates::write_in(
                ctx,
                &dir,
                &format!(
                    "{kind}_{}.{}",
                    lang.to_lowercase().replace(' ', "_"),
                    format.ext()
                ),
                &format.render(&combined)?,
            )?);
        }
        for l in &letter_defs {
            files.push(templates::write_in(
                ctx,
                &dir,
                &format!("template_english_{}.md", l.kind),
                l.markdown.as_bytes(),
            )?);
        }
        files_written.extend(files.iter().cloned());
        letters = Some(LettersWritten {
            source_file: lt.label.clone(),
            folder: shown,
            households,
            letters: n_letters,
            files,
            by_language: by_lang
                .into_iter()
                .map(
                    |(language, (documents, agency_translation))| LanguageCount {
                        language,
                        documents,
                        agency_translation,
                    },
                )
                .collect(),
            english_fallback: fallback,
            blank_fields,
            fields_found: found,
            unknown_placeholders: unknown.into_iter().collect(),
        });
        local_source = Some(lt.source());
    }

    let kinds: Vec<String> = sheet_out
        .iter()
        .map(|s| s.template_kind.clone())
        .chain(letter_defs.iter().map(|l| l.kind.clone()))
        .collect();
    let kinds_ref: Vec<&str> = kinds.iter().map(String::as_str).collect();
    let how_to_add_translations = templates::translation_help(&kinds_ref);

    let regulation = format!(
        "{} (effective date used: {})",
        c.meta.source.title,
        c.meta.effective_date.clone().unwrap_or_default()
    );
    let s = summary(
        &c,
        &regulation,
        &conditions,
        &inspection,
        &board_of_health,
        &tenant_protections,
        &local,
        &sheet_out,
        &sheet_folder,
        &letters,
    );

    caveats.push(
        c.legal_notice
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    );
    caveats.push(
        "Local bylaws, the Boston and other city housing codes, and the State Building Code may \
         add requirements; the board of health enforces 105 CMR 410.000 locally."
            .into(),
    );
    caveats.extend(content::review_caveats(&[HOUSING_CONDITIONS]));
    let mut keys: Vec<&str> = vec!["cmr410", "dph-olrr"];
    for d in &picked {
        for x in d.health_effects.iter().chain(&d.advice) {
            keys.push(x.source.as_str());
        }
    }
    for p in &c.tenant_protections {
        keys.push(p.source.as_str());
    }
    for s in &sheets {
        for k in &s.sources {
            keys.push(k.as_str());
        }
    }
    let mut all_sources = content::sources_for(keys, &c.sources);
    all_sources.extend(sources);
    if let Some(ls) = local_source {
        all_sources.push(ls);
    }

    let mut notes = Vec::new();
    if args.households_path.is_some() {
        notes.push(
            "Household details from your file were written into the letters in /data/output only; \
             the response has counts and file names only."
                .into(),
        );
    }
    Ok(Envelope::new(
        s,
        HousingGuideResult {
            regulation,
            conditions,
            unmatched_terms: unmatched,
            inspection,
            board_of_health,
            tenant_protections,
            legal_notice: c
                .legal_notice
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            local_housing: local,
            teaching_sheets: sheet_out,
            teaching_sheet_folder: sheet_folder,
            letter_templates,
            letters,
            how_to_add_translations,
        },
    )
    .sources(all_sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        identifiers_detected: identifiers,
        files_written,
        notes,
        ..Privacy::none()
    })
    .decision_support())
}

fn pct(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.1}%")).unwrap_or_else(|| "—".into())
}

#[allow(clippy::too_many_arguments)]
fn summary(
    c: &HousingContent,
    regulation: &str,
    conditions: &[ConditionGuide],
    inspection: &InspectionGuide,
    boh: &BoardOfHealth,
    protections: &[ProtectionOut],
    local: &Option<LocalHousing>,
    sheets: &[SheetOut],
    sheet_folder: &Option<String>,
    letters: &Option<LettersWritten>,
) -> String {
    let mut s = String::from("**Housing conditions and health — State Sanitary Code**\n\n");
    s.push_str(&format!("Regulation: {regulation}.\n\n"));
    for d in conditions {
        s.push_str(&format!(
            "### {}{}\n\n",
            d.label,
            if d.urgent {
                " — always deemed to endanger health (410.630(A))"
            } else {
                ""
            }
        ));
        for k in &d.code {
            s.push_str(&format!("- **{}** {}\n", k.section, k.text));
        }
        if !d.owner.is_empty() {
            s.push_str(&format!("\nOwner: {}\n", d.owner.join(" ")));
        }
        if !d.occupant.is_empty() {
            s.push_str(&format!("\nOccupant: {}\n", d.occupant.join(" ")));
        }
        if !d.health_effects.is_empty() {
            s.push_str("\nHealth effects:\n");
            for h in &d.health_effects {
                s.push_str(&content::bullet(h));
            }
        }
        if !d.at_risk.is_empty() {
            s.push_str(&format!("\nMost at risk: {}.\n", d.at_risk.join(", ")));
        }
        if !d.advice.is_empty() {
            s.push_str("\nAdvice:\n");
            for h in &d.advice {
                s.push_str(&content::bullet(h));
            }
        }
        s.push('\n');
    }
    s.push_str("### Getting a board of health inspection\n\n");
    for x in inspection
        .request
        .iter()
        .chain(&inspection.during)
        .chain(&inspection.after)
        .chain(&inspection.hearings)
    {
        s.push_str(&content::bullet(x));
    }
    s.push_str(&format!(
        "\nConditions always deemed to endanger health (inspection within 1 business day; good \
         faith correction within 24 hours): {}\n\n",
        inspection.urgent_conditions.join("; ")
    ));
    match (&boh.town, &boh.phone) {
        (Some(t), Some(p)) => s.push_str(&format!("Board of health ({t}): {p}.\n\n")),
        _ => s.push_str(&format!("{}\n\n", boh.how_to_find)),
    }
    s.push_str("### Tenant protections (decision support, not legal advice)\n\n");
    for p in protections {
        s.push_str(&format!(
            "- **{}:** {} _({})_\n",
            p.label, p.text, p.citation
        ));
    }
    s.push_str(&format!(
        "\n_{}_\n\n",
        c.legal_notice
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    ));
    if let Some(l) = local {
        s.push_str(&format!(
            "**Housing in {}** (ACS 5-year): built before 1980 {}, before 1950 {}; \
             renter-occupied {}; renters paying 30% or more of income for housing {}; crowded \
             (more than 1 person per room) {}.\n\n",
            l.area,
            pct(l.built_before_1980_pct),
            pct(l.built_before_1950_pct),
            pct(l.renter_pct),
            pct(l.renter_cost_burden_pct),
            pct(l.crowding_pct)
        ));
    }
    if !sheets.is_empty() {
        let rows: Vec<Vec<String>> = sheets
            .iter()
            .map(|x| {
                vec![
                    x.title.clone(),
                    format!("{:.1}", x.reading_grade),
                    if x.meets_target { "yes" } else { "no" }.into(),
                    x.file.clone().unwrap_or_else(|| "not written".into()),
                    x.translations
                        .iter()
                        .map(|t| {
                            format!(
                                "{}: {}",
                                t.language,
                                if t.file.is_some() {
                                    "agency translation"
                                } else {
                                    "none"
                                }
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("; "),
                ]
            })
            .collect();
        s.push_str("**Tenant teaching sheets** (English; reading grade by the COMM-01 engine)\n\n");
        s.push_str(&md::table(
            &["Sheet", "Grade", "Meets target", "File", "Translations"],
            &rows,
        ));
        if let Some(f) = sheet_folder {
            s.push_str(&format!("\nWritten to `{f}`.\n"));
        }
        s.push('\n');
    }
    match letters {
        Some(l) => s.push_str(&format!(
            "**Letters** filled from `{}`: {} household(s), {} letter(s) written to `{}` \
             ({} blank field(s) left to fill in by hand). Nothing from the file is shown here.\n",
            l.source_file, l.households, l.letters, l.folder, l.blank_fields
        )),
        None => s.push_str(
            "**Letters:** inspection request and repair request templates are in the result; give \
             households_path (a CSV/XLSX in /data) to fill them locally into /data/output.\n",
        ),
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> HousingContent {
        content::load(HOUSING_CONDITIONS).unwrap()
    }

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn content_loads_and_cites_known_sources() {
        let c = content();
        assert!(c.conditions.len() >= 7);
        for d in &c.conditions {
            for x in d.health_effects.iter().chain(&d.advice) {
                assert!(c.sources.contains_key(&x.source), "{} {}", d.id, x.source);
            }
            assert!(!d.code.is_empty());
        }
        for x in c
            .inspection
            .request
            .iter()
            .chain(&c.inspection.during)
            .chain(&c.inspection.after)
            .chain(&c.inspection.hearings)
        {
            assert!(c.sources.contains_key(&x.source), "{}", x.source);
        }
        for p in &c.tenant_protections {
            assert!(c.sources.contains_key(&p.source), "{}", p.source);
        }
        for s in &c.teaching_sheets {
            for k in &s.sources {
                assert!(c.sources.contains_key(k), "{k}");
            }
            if let Some(id) = &s.condition {
                assert!(c.conditions.iter().any(|d| &d.id == id), "{id}");
            }
        }
        assert_eq!(c.urgent_list.items.len(), 18);
        // The letter placeholders are the ones the household file fills.
        for f in HOUSEHOLD_FIELDS {
            assert!(c.letters.placeholders.iter().any(|p| p == f.0), "{}", f.0);
        }
    }

    #[test]
    fn teaching_sheets_meet_the_reading_target() {
        let c = content();
        let mut values = BTreeMap::new();
        values.insert(
            "board_of_health_phone".to_string(),
            "413-555-0199".to_string(),
        );
        values.insert("town".to_string(), "Amherst".to_string());
        for s in &c.teaching_sheets {
            let (filled, unknown) = templates::fill(&s.markdown, &values, BLANK);
            assert!(unknown.is_empty(), "{}: {unknown:?}", s.id);
            let a = analyze(&filled, &AnalyzeOptions::default());
            assert!(
                a.grade.median_grade <= default_target_grade(),
                "{} reads at grade {}",
                s.id,
                a.grade.median_grade
            );
        }
    }

    #[test]
    fn conditions_match_on_words() {
        let c = content();
        let ids = |terms: &[&str]| {
            let t: Vec<String> = terms.iter().map(|s| s.to_string()).collect();
            let (m, u) = match_conditions(&c, &t);
            (m.iter().map(|d| d.id.clone()).collect::<Vec<_>>(), u)
        };
        let (m, u) = ids(&["roaches and a leak"]);
        assert!(m.contains(&"pests".to_string()) && m.contains(&"moisture_mold".to_string()));
        assert!(u.is_empty());
        let (m, _) = ids(&["no heat"]);
        assert_eq!(m, vec!["heat"]);
        let (m, u) = ids(&["lead paint", "zebras"]);
        assert_eq!(m, vec!["lead"]);
        assert_eq!(u, vec!["zebras"]);
        // "heater" must not match "heat" by substring alone, but it is a keyword.
        let (m, _) = ids(&["space heater"]);
        assert_eq!(m, vec!["heat"]);
        // "leading" is not "lead".
        let (m, u) = ids(&["leading"]);
        assert!(m.is_empty() && u == vec!["leading"]);
        assert_eq!(ids(&[]).0.len(), c.conditions.len());
    }

    #[test]
    fn guide_writes_sheets_and_uses_agency_translations_only() {
        let (dir, ctx) = ctx();
        let tdir = dir.path().join("data/templates/spanish");
        std::fs::create_dir_all(&tdir).unwrap();
        std::fs::write(
            tdir.join("housing_heat.md"),
            "# Sin calefacción\n\nLlame a la junta de salud al {{board_of_health_phone}}.\n",
        )
        .unwrap();
        let env = housing_conditions_health_guide(
            &ctx,
            HousingGuideArgs {
                conditions: vec!["no heat".into()],
                area: Some("Springfield".into()),
                languages: vec!["Spanish".into(), "Portuguese".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let d = &env.data;
        assert_eq!(d.conditions.len(), 1);
        assert!(d.conditions[0].urgent);
        assert!(
            d.conditions[0]
                .code
                .iter()
                .any(|k| k.section == "410.180(A)")
        );
        // Inspection sheet plus the heat sheet.
        assert_eq!(d.teaching_sheets.len(), 2);
        let heat = d.teaching_sheets.iter().find(|s| s.id == "heat").unwrap();
        assert!(heat.meets_target);
        let es = heat
            .translations
            .iter()
            .find(|t| t.language == "Spanish")
            .unwrap();
        assert!(es.file.is_some());
        let pt = heat
            .translations
            .iter()
            .find(|t| t.language == "Portuguese")
            .unwrap();
        assert!(pt.file.is_none());
        assert!(
            env.caveats
                .iter()
                .any(|c| c.contains("does not machine-translate"))
        );
        let l = d.local_housing.as_ref().unwrap();
        assert!(l.built_before_1980_pct.unwrap() > 50.0);
        assert!(env.notice.is_some());
        assert!(d.tenant_protections.iter().any(|p| p.id == "retaliation"));
        assert_eq!(d.letter_templates.len(), 2);
        assert!(d.letters.is_none());
        for f in &env.privacy.files_written {
            assert!(f.starts_with("output/"), "{f}");
        }
        // No match is an error; letters need a file.
        assert!(
            housing_conditions_health_guide(
                &ctx,
                HousingGuideArgs {
                    conditions: vec!["zebras".into()],
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            housing_conditions_health_guide(
                &ctx,
                HousingGuideArgs {
                    letters: Some("both".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn letters_are_filled_locally_and_not_returned() {
        let (dir, ctx) = ctx();
        std::fs::write(
            dir.path().join("data/households.csv"),
            "Name,Address,Unit,Town,Phone,Landlord,Problems,Date noticed,Language\n\
             Ottilie Fernsby,12 Larkspur Way,3B,Holyoke,413-555-0142,Quorum Realty,No heat and mice,2026-11-01,Spanish\n\
             Bartholomew Quince,48 Juniper Row,,Holyoke,,Quorum Realty,Leak in bathroom ceiling,,English\n",
        )
        .unwrap();
        let env = housing_conditions_health_guide(
            &ctx,
            HousingGuideArgs {
                conditions: vec!["heat".into(), "pests".into()],
                households_path: Some("households.csv".into()),
                teaching_sheets: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        let l = env.data.letters.as_ref().unwrap();
        assert_eq!(l.households, 2);
        assert_eq!(l.letters, 4);
        assert!(l.blank_fields >= 2);
        assert_eq!(l.english_fallback, 2);
        let json = serde_json::to_string(&env).unwrap();
        for secret in [
            "Ottilie",
            "Fernsby",
            "12 Larkspur Way",
            "413-555-0142",
            "Quince",
            "Juniper",
        ] {
            assert!(!json.contains(secret), "{secret} leaked");
        }
        assert!(
            env.privacy
                .identifiers_detected
                .iter()
                .any(|i| !i.is_empty())
        );
        let out = dir.path().join("data/output");
        let folder = out.join(l.folder.rsplit('/').next().unwrap());
        assert!(folder.exists());
        assert!(env.data.teaching_sheets.iter().all(|s| s.file.is_none()));
    }
}
