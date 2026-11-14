//! EQ-08 `community_health_assessment_builder`: guides a community health
//! assessment with MAPP 2.0 (NACCHO) and drafts report sections.
//!
//! The tool assembles what already exists in this server as library calls (the
//! EQ-01 community profile with PLACES and the Social Vulnerability Index,
//! EQ-03 language access, and the Phase 14 environmental justice, climate,
//! food access, drinking water, and fluoridation tools) into the community
//! status assessment; lays out the community context and community partner
//! assessments with prompts and a sector checklist (a partner roster file the
//! nurse keeps in /data is counted by sector); and brings in what nearby
//! hospitals' community health needs assessments (CHNAs) found, reading CHNA
//! reports the nurse saves in /data and extracting their priority lists with
//! page citations. A DOCX draft goes to /data/output.
//!
//! MAPP 2.0 was verified against journal articles, not NACCHO's own pages
//! (naccho.org was unreachable): see `content/equity/cha_framework.yaml`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_local::extract::{FileKind, Section, extract};
use phn_local::phi::{self, Identifier, RedactionLevel};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::language::{LanguageAccessArgs, LanguageAccessResult, language_access_profile};
use super::profile::{ProfileArgs, ProfileResult, build_community_health_profile};
use super::svi::{SviArgs, SviResult, get_social_vulnerability_index};
use crate::chronic::fluoridation::{FluoridationArgs, community_water_fluoridation_status};
use crate::chronic::food_access::{FoodAccessArgs, food_access_mapper};
use crate::context::Context;
use crate::env::climate_profile::{
    ClimateProfileArgs, ClimateProfileResult, climate_health_vulnerability_profile,
};
use crate::env::drinking_water::{
    DrinkingWaterArgs, DrinkingWaterResult, drinking_water_quality_lookup,
};
use crate::env::ej_screen::{EjScreenArgs, environmental_justice_screen};
use crate::epr::area::{self, Area, AreaLevel};
use crate::epr::content::{self, Cited, CitedOut, Confidence, DocSource};
use crate::md;
use crate::school::templates::{self, DocFormat};

/// Content id of `content/equity/cha_framework.yaml`.
pub const CHA_FRAMEWORK: &str = "equity-cha-framework";

/// Snapshot of Massachusetts hospitals (CMS).
const HOSPITALS_SNAPSHOT: &str = "cms-ma-hospitals";
const HOSPITALS_FILE: &str = "ma_hospitals.csv";

/// Most CHNA files read in one call.
const MAX_CHNA_FILES: usize = 20;
/// Largest CHNA file (bytes).
const MAX_FILE_BYTES: u64 = 40 * 1024 * 1024;
/// Most priority lines kept per report.
const MAX_PRIORITIES: usize = 15;
/// Longest priority line (characters).
const MAX_PRIORITY_CHARS: usize = 160;
/// Longest section excerpt kept in the response (characters).
const EXCERPT_CHARS: usize = 1_800;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PhaseDef {
    number: u32,
    name: String,
    summary: String,
    activities: Vec<Cited>,
    confidence: Confidence,
    source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct AssessmentDef {
    id: String,
    name: String,
    data: String,
    purpose: String,
    server_tools: Vec<String>,
    source: String,
    section: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FrameworkDef {
    name: String,
    owner: String,
    released: String,
    source_status: String,
    phases: Vec<PhaseDef>,
    assessments: Vec<AssessmentDef>,
    partner_assessment_facts: Vec<Cited>,
    equity_note: Cited,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SectorDef {
    id: String,
    label: String,
    keywords: Vec<String>,
    why: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DirectoryDef {
    title: String,
    url: String,
    what: String,
    source: String,
    confidence: Confidence,
    note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ThemeDef {
    id: String,
    label: String,
    keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ChnaDef {
    federal: Vec<Cited>,
    massachusetts: Vec<Cited>,
    elements: Vec<String>,
    directory: Vec<DirectoryDef>,
    how_to_collect: Vec<String>,
    theme_vocabulary: Vec<ThemeDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct OutlineDef {
    id: String,
    title: String,
    guidance: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ChaContent {
    sources: BTreeMap<String, DocSource>,
    framework: FrameworkDef,
    partner_sectors: Vec<SectorDef>,
    context_prompts: Vec<String>,
    context_how_to: Vec<Cited>,
    chna: ChnaDef,
    report_outline: Vec<OutlineDef>,
    not_verified: Vec<String>,
}

// ---------------------------------------------------------------- arguments

/// Arguments for `community_health_assessment_builder`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CommunityAssessmentArgs {
    /// A Massachusetts city or town, or a county, to assess. Default: your saved town. (A
    /// county gets everything except the drinking water and fluoridation sections, which are
    /// by town.)
    pub area: Option<String>,
    /// Data sections to assemble for the community status assessment (default: all):
    /// "profile" (EQ-01 with CDC PLACES and the Social Vulnerability Index), "language" (EQ-03),
    /// "social_vulnerability" (EQ-02), "environmental_justice", "climate", "food_access",
    /// "drinking_water", "fluoridation". Each section that cannot be built is reported, and the
    /// rest are still returned.
    pub sections: Vec<String>,
    /// Names of hospitals that serve your community (optional). Hospitals in your town and
    /// county are listed from CMS data either way; naming hospitals here adds them to the list
    /// of CHNAs to collect.
    pub hospitals: Vec<String>,
    /// Hospital CHNA reports you saved in /data (PDF, DOCX, HTML, or TXT files, or a folder of
    /// them, at most 20). The tool finds each report's priority list and cites the page. CHNAs
    /// are public documents; a file that holds client records is refused.
    pub chna_paths: Vec<String>,
    /// Your partner roster (CSV or XLSX in /data) with one organization per row: an organization
    /// name column and a sector or type column. Counted by sector against the checklist; contact
    /// people in the file are never read into the response. A list of clients is refused.
    pub partners_path: Option<String>,
    /// Write the draft report sections to /data/output (default true).
    pub write_document: Option<bool>,
    /// Document format: "docx" (default) or "pdf".
    pub format: Option<DocFormat>,
}

// ---------------------------------------------------------------- results

/// The area assessed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AreaOut {
    pub name: String,
    pub level: AreaLevel,
    pub county: Option<String>,
}

/// One MAPP 2.0 phase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PhaseOut {
    pub number: u32,
    pub name: String,
    pub summary: String,
    pub activities: Vec<CitedOut>,
    pub confidence: Confidence,
}

/// One MAPP 2.0 assessment and how this server's tools feed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AssessmentOut {
    pub name: String,
    pub data: String,
    pub purpose: String,
    /// This server's tools that supply data for it (this project's mapping, not NACCHO's).
    pub server_tools: Vec<String>,
    pub confidence: Confidence,
    pub citation: String,
}

/// The framework.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FrameworkOut {
    pub name: String,
    pub owner: String,
    pub released: String,
    /// How MAPP 2.0 was verified (NACCHO's pages could not be read).
    pub source_status: String,
    pub phases: Vec<PhaseOut>,
    pub assessments: Vec<AssessmentOut>,
    pub equity_note: CitedOut,
}

/// One data section of the community status assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StatusSection {
    pub id: String,
    pub title: String,
    /// The tool the section comes from.
    pub tool: String,
    /// "included", "unavailable" (with the reason), "not requested", or "not for this level".
    pub status: String,
    /// Why a section is unavailable or left out.
    pub detail: Option<String>,
    /// Key facts.
    pub headline: Vec<String>,
    /// The start of the tool's own summary (the full tables are in the document).
    pub excerpt: Option<String>,
    /// "live", "cache", "snapshot", "computed", as the tool reported.
    pub data_modes: Vec<String>,
}

/// The community status assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StatusAssessment {
    pub sections: Vec<StatusSection>,
    /// Quantitative data this server cannot supply for the assessment.
    pub gaps: Vec<String>,
}

/// A row of the partner sector checklist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SectorRow {
    pub sector: String,
    pub why: String,
    /// Organizations in your roster that match (null when no roster was given).
    pub partners: Option<usize>,
}

/// Summary of a partner roster (counts only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RosterSummary {
    pub source_file: String,
    pub organizations: usize,
    /// Rows whose type matched no sector.
    pub unmatched: usize,
    pub sectors_covered: usize,
    pub sectors_total: usize,
    /// Sectors with no organization.
    pub missing_sectors: Vec<String>,
}

/// The community partner assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PartnerAssessment {
    pub facts: Vec<CitedOut>,
    pub sector_checklist: Vec<SectorRow>,
    pub roster: Option<RosterSummary>,
    pub steps: Vec<String>,
}

/// The community context assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContextAssessment {
    pub purpose: String,
    pub prompts: Vec<String>,
    pub how_to: Vec<CitedOut>,
}

/// A hospital in or near the area (CMS directory data).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HospitalRow {
    pub name: String,
    pub town: String,
    pub county: String,
    pub hospital_type: String,
    /// CMS's ownership field.
    pub ownership: String,
    /// "in the town", "in the county", or "named by you".
    pub relation: String,
    /// True for a non-profit acute care, critical access, or children's hospital, which files
    /// community benefits reports with the Attorney General (a likely, not certain, test).
    pub likely_files_community_benefits_report: bool,
}

/// A directory link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DirectoryLink {
    pub title: String,
    pub url: String,
    pub what: String,
    pub confidence: Confidence,
    pub note: String,
}

/// A priority line found in a CHNA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PriorityLine {
    /// The line as written in the report (identifiers removed), shortened.
    pub text: String,
    /// "page N" or the heading it was under.
    pub location: String,
    /// Theme labels matched by this project's word list.
    pub themes: Vec<String>,
}

/// What was read from one CHNA file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChnaReportOut {
    pub id: String,
    /// The file as you gave it (a path in /data).
    pub file: String,
    pub pages: Option<usize>,
    /// The most recent year (2015-2030) on the first pages: usually the report's year.
    pub year_seen: Option<i32>,
    /// Places in the report where a priority or significant needs list was recognized.
    pub priority_sections_found: usize,
    pub priorities: Vec<PriorityLine>,
    /// Theme labels found in this report's priority lines.
    pub themes: Vec<String>,
    pub note: String,
}

/// A theme across the CHNAs read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ThemeRow {
    pub theme: String,
    /// CHNA reports (of those read) whose priority lines mention it.
    pub reports: usize,
}

/// Hospital community health needs assessments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HospitalSection {
    pub hospitals: Vec<HospitalRow>,
    pub federal_requirements: Vec<CitedOut>,
    pub massachusetts_guidance: Vec<CitedOut>,
    pub required_elements: Vec<String>,
    pub directory: Vec<DirectoryLink>,
    pub how_to_collect: Vec<String>,
    pub reports_read: Vec<ChnaReportOut>,
    /// Themes by how many of the reports read mention them (empty without CHNA files).
    pub common_themes: Vec<ThemeRow>,
    /// A statewide reference for comparison.
    pub statewide_focus_issues: Vec<String>,
}

/// Result of `community_health_assessment_builder`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CommunityAssessmentResult {
    pub area: AreaOut,
    pub framework: FrameworkOut,
    pub status_assessment: StatusAssessment,
    pub context_assessment: ContextAssessment,
    pub partner_assessment: PartnerAssessment,
    pub hospital_chnas: HospitalSection,
    /// Next steps in the workflow, with the tool to use.
    pub next_steps: Vec<String>,
    /// What could not be verified.
    pub not_verified: Vec<String>,
    /// The draft report, when written.
    pub document: Option<String>,
}

// ---------------------------------------------------------------- gathering (async part)

type Part<T> = Option<std::result::Result<Envelope<T>, String>>;

/// What the async data tools returned (errors as text, so one failure does not stop the rest).
pub struct Gathered {
    area: Area,
    wanted: BTreeSet<String>,
    profile: Part<ProfileResult>,
    language: Part<LanguageAccessResult>,
    climate: Part<ClimateProfileResult>,
    water: Part<DrinkingWaterResult>,
}

const ALL_SECTIONS: [&str; 8] = [
    "profile",
    "language",
    "social_vulnerability",
    "environmental_justice",
    "climate",
    "food_access",
    "drinking_water",
    "fluoridation",
];

fn wanted_sections(args: &CommunityAssessmentArgs) -> Result<BTreeSet<String>> {
    if args.sections.is_empty() {
        return Ok(ALL_SECTIONS.iter().map(|s| s.to_string()).collect());
    }
    let mut out = BTreeSet::new();
    for s in &args.sections {
        let n = phn_core::text::normalize(&s.replace('_', " "));
        let found = ALL_SECTIONS
            .iter()
            .find(|k| phn_core::text::normalize(&k.replace('_', " ")) == n);
        match found {
            Some(k) => {
                out.insert(k.to_string());
            }
            None => {
                return Err(PhnError::InvalidInput(format!(
                    "unknown section '{}'; sections are: {}",
                    s.trim(),
                    ALL_SECTIONS.join(", ")
                )));
            }
        }
    }
    Ok(out)
}

/// Run the data tools that need the network (live data, then cache, then snapshot).
pub async fn gather(ctx: &Context, args: &CommunityAssessmentArgs) -> Result<Gathered> {
    let wanted = wanted_sections(args)?;
    let area = area::resolve(ctx, args.area.as_deref(), false)?;
    if area.level == AreaLevel::State {
        return Err(PhnError::InvalidInput(
            "name a Massachusetts city, town, or county to assess (or save your town with \
             configure_jurisdiction)"
                .into(),
        ));
    }
    let name = area.name.clone();
    let want = |k: &str| wanted.contains(k);
    let profile = async {
        if want("profile") {
            Some(
                build_community_health_profile(
                    ctx,
                    ProfileArgs {
                        area: Some(name.clone()),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| e.to_string()),
            )
        } else {
            None
        }
    };
    let language = async {
        if want("language") {
            Some(
                language_access_profile(
                    ctx,
                    LanguageAccessArgs {
                        area: Some(name.clone()),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| e.to_string()),
            )
        } else {
            None
        }
    };
    let climate = async {
        if want("climate") {
            Some(
                climate_health_vulnerability_profile(
                    ctx,
                    ClimateProfileArgs {
                        area: Some(name.clone()),
                        document: Some(false),
                    },
                )
                .await
                .map_err(|e| e.to_string()),
            )
        } else {
            None
        }
    };
    let town_level = area.level == AreaLevel::Town;
    let water = async {
        if want("drinking_water") && town_level {
            Some(
                drinking_water_quality_lookup(
                    ctx,
                    DrinkingWaterArgs {
                        town: Some(name.clone()),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| e.to_string()),
            )
        } else {
            None
        }
    };
    let (profile, language, climate, water) = tokio::join!(profile, language, climate, water);
    Ok(Gathered {
        area,
        wanted,
        profile,
        language,
        climate,
        water,
    })
}

/// Gather and assemble in one call (for tests and library use).
pub async fn community_health_assessment_builder(
    ctx: &Context,
    args: CommunityAssessmentArgs,
) -> Result<Envelope<CommunityAssessmentResult>> {
    let g = gather(ctx, &args).await?;
    assemble(ctx, args, g)
}

// ---------------------------------------------------------------- CHNA reading

static PRIORITY_ANCHOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(priorit(?:y|ies|ized|ization)\b|significant (?:community )?health needs?|top (?:\w+ ){0,2}(?:health )?needs?)",
    )
    .unwrap()
});
static BULLET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:[•●▪◦○·*\-–—]|\d{1,2}[.)]|[A-Za-z][.)])\s+(?P<t>\S.*)$").unwrap()
});
/// A bullet or number alone on a line (PDF text often puts the mark and the item on two lines).
static LONE_BULLET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:[•●▪◦○·*\-–—]|\d{1,2}[.)]|[A-Za-z][.)])\s*$").unwrap());
/// A page footer such as "Page 2 of 10".
static PAGE_FOOTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*page\s+\d+(?:\s+of\s+\d+)?\s*$").unwrap());
static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(20[12]\d|2030)\b").unwrap());

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Shorten at a character boundary.
fn clip(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= max {
        return t.to_string();
    }
    let cut: String = t.chars().take(max).collect();
    format!("{}…", cut.trim_end())
}

fn themes_of(text: &str, vocab: &[ThemeDef]) -> Vec<String> {
    let lower = text.to_lowercase();
    vocab
        .iter()
        .filter(|t| t.keywords.iter().any(|k| lower.contains(&k.to_lowercase())))
        .map(|t| t.label.clone())
        .collect()
}

/// Find priority lines in the sections of a report. Returns the lines (with location and
/// themes) and the number of places where a list was recognized.
fn extract_priorities(sections: &[Section], vocab: &[ThemeDef]) -> (Vec<PriorityLine>, usize) {
    let mut out: Vec<PriorityLine> = Vec::new();
    let mut anchors = 0usize;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for sec in sections {
        let location = match sec.page {
            Some(p) => format!("page {p}"),
            None if !sec.heading.trim().is_empty() => {
                format!("section \"{}\"", clip(&sec.heading, 60))
            }
            None => "text".to_string(),
        };
        let lines: Vec<&str> = sec.text.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            if !PRIORITY_ANCHOR.is_match(line) || word_count(line) > 30 && !line.contains(':') {
                i += 1;
                continue;
            }
            // An anchor: an inline list after a colon, then bullet or short lines after it.
            let mut found: Vec<String> = Vec::new();
            if let Some((_, rest)) = line.split_once(':') {
                let parts: Vec<String> = rest
                    .split([';', '•'])
                    .map(|p| p.trim().trim_end_matches('.').trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect();
                if (2..=10).contains(&parts.len())
                    && parts
                        .iter()
                        .all(|p| word_count(p) <= 10 && p.chars().count() <= 90)
                {
                    found.extend(parts);
                }
            }
            let mut j = i + 1;
            let mut bullets = 0usize;
            let mut gap = 0usize;
            let mut lone_bullet = false;
            while j < lines.len() && found.len() < MAX_PRIORITIES && j < i + 40 {
                let l = lines[j].trim();
                if PAGE_FOOTER.is_match(l) {
                    break;
                }
                if LONE_BULLET.is_match(l) {
                    lone_bullet = true;
                    j += 1;
                    continue;
                }
                if lone_bullet && !l.is_empty() {
                    lone_bullet = false;
                    if word_count(l) <= 14 {
                        found.push(l.trim_end_matches([';', '.']).to_string());
                        bullets += 1;
                        j += 1;
                        continue;
                    }
                    break;
                }
                if l.is_empty() {
                    gap += 1;
                    if gap > 1 && bullets > 0 {
                        break;
                    }
                    j += 1;
                    continue;
                }
                gap = 0;
                if let Some(c) = BULLET.captures(l) {
                    let t = c["t"].trim().trim_end_matches([';', '.']).to_string();
                    if word_count(&t) <= 14 {
                        found.push(t);
                        bullets += 1;
                    } else {
                        break;
                    }
                } else if bullets == 0
                    && word_count(l) <= 8
                    && l.chars().next().is_some_and(char::is_uppercase)
                    && !l.ends_with('.')
                    && !PRIORITY_ANCHOR.is_match(l)
                {
                    // Plain short heading-like lines (a list without bullet marks).
                    found.push(l.to_string());
                } else {
                    break;
                }
                j += 1;
            }
            if !found.is_empty() {
                anchors += 1;
                for f in found {
                    let red = phi::redact_text(&f, RedactionLevel::Strict).text;
                    if red.contains("[REDACTED") {
                        continue;
                    }
                    let text = clip(&red, MAX_PRIORITY_CHARS);
                    let key = text.to_lowercase();
                    if seen.insert(key) {
                        out.push(PriorityLine {
                            themes: themes_of(&text, vocab),
                            text,
                            location: location.clone(),
                        });
                    }
                }
            }
            i = j.max(i + 1);
        }
    }
    out.truncate(MAX_PRIORITIES * 3);
    (out, anchors)
}

/// Refuse documents that hold client records rather than a public report.
fn refuse_if_client_record(text: &str) -> Result<Vec<String>> {
    let findings = phi::scan_text(text);
    let structured = findings
        .iter()
        .filter(|f| {
            f.confidence == phi::Confidence::High
                && matches!(
                    f.kind,
                    Identifier::Ssn
                        | Identifier::MedicalRecordNumber
                        | Identifier::HealthPlanNumber
                        | Identifier::AccountNumber
                        | Identifier::BirthDate
                )
        })
        .count();
    if structured >= 3 {
        return Err(PhnError::Privacy(
            "this file holds several record numbers, social security numbers, or dates of birth: \
             it looks like a client record, not a published CHNA report, so it was not read. \
             Save only the hospital's public CHNA report."
                .into(),
        ));
    }
    let mut kinds: Vec<String> = findings
        .iter()
        .filter(|f| f.confidence == phi::Confidence::High)
        .map(|f| f.kind.label().to_string())
        .collect();
    kinds.sort();
    kinds.dedup();
    Ok(kinds)
}

fn read_chna_reports(
    ctx: &Context,
    paths: &[String],
    vocab: &[ThemeDef],
) -> Result<(Vec<ChnaReportOut>, Vec<Source>, Vec<String>)> {
    let sandbox = ctx.sandbox()?;
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for p in paths {
        let resolved = sandbox.resolve_existing(p)?;
        if resolved.is_dir() {
            let mut inside: Vec<std::path::PathBuf> = std::fs::read_dir(&resolved)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|f| {
                    f.is_file()
                        && matches!(
                            FileKind::of(f),
                            FileKind::Pdf
                                | FileKind::Docx
                                | FileKind::Html
                                | FileKind::Text
                                | FileKind::Markdown
                        )
                })
                .collect();
            inside.sort();
            files.extend(inside);
        } else {
            files.push(resolved);
        }
    }
    if files.is_empty() {
        return Err(PhnError::InvalidInput(
            "no CHNA files were found: chna_paths should name PDF, DOCX, HTML, or TXT files (or a \
             folder of them) in /data"
                .into(),
        ));
    }
    if files.len() > MAX_CHNA_FILES {
        return Err(PhnError::InvalidInput(format!(
            "{} CHNA files found; read at most {MAX_CHNA_FILES} at a time",
            files.len()
        )));
    }
    let mut reports = Vec::new();
    let mut sources = Vec::new();
    let mut identifiers: Vec<String> = Vec::new();
    for (k, f) in files.iter().enumerate() {
        let label = sandbox.display_path(f);
        match FileKind::of(f) {
            FileKind::Pdf
            | FileKind::Docx
            | FileKind::Html
            | FileKind::Text
            | FileKind::Markdown => {}
            _ => {
                return Err(PhnError::Unsupported(format!(
                    "'{label}' is not a document: a CHNA report is a PDF, DOCX, HTML, or TXT file. \
                     Spreadsheets and data files are not read here."
                )));
            }
        }
        if std::fs::metadata(f)?.len() > MAX_FILE_BYTES {
            return Err(PhnError::InvalidInput(format!(
                "'{label}' is larger than {} MB",
                MAX_FILE_BYTES / 1024 / 1024
            )));
        }
        let ex = extract(f)?;
        let kinds = refuse_if_client_record(&ex.text)?;
        identifiers.extend(kinds);
        let (priorities, anchors) = extract_priorities(&ex.sections, vocab);
        let head: String = ex.text.chars().take(4000).collect();
        let year = YEAR
            .find_iter(&head)
            .filter_map(|m| m.as_str().parse::<i32>().ok())
            .max();
        let mut themes: Vec<String> = priorities
            .iter()
            .flat_map(|p| p.themes.iter().cloned())
            .collect();
        themes.sort();
        themes.dedup();
        let note = if priorities.is_empty() {
            "No priority or significant-needs list was recognized (the report may use another \
             layout, or be scanned images). Open its executive summary and read the priorities \
             yourself."
                .to_string()
        } else {
            "Lines were machine-extracted from the report's priority sections and can be \
             incomplete or include nearby headings: check each against the report."
                .to_string()
        };
        let mut notes = ex.notes.clone();
        if !notes.is_empty() {
            let mut n = note;
            n.push(' ');
            n.push_str(&notes.join(" "));
            notes.clear();
            reports.push(ChnaReportOut {
                id: format!("C{}", k + 1),
                file: label.clone(),
                pages: ex.pages,
                year_seen: year,
                priority_sections_found: anchors,
                priorities,
                themes,
                note: n,
            });
        } else {
            reports.push(ChnaReportOut {
                id: format!("C{}", k + 1),
                file: label.clone(),
                pages: ex.pages,
                year_seen: year,
                priority_sections_found: anchors,
                priorities,
                themes,
                note,
            });
        }
        sources.push(Source::local_file(label));
    }
    identifiers.sort();
    identifiers.dedup();
    Ok((reports, sources, identifiers))
}

// ---------------------------------------------------------------- partner roster

struct Roster {
    label: String,
    rows: Vec<(String, String)>, // (organization, sector text)
}

fn load_roster(ctx: &Context, path: &str) -> Result<Roster> {
    let sandbox = ctx.sandbox()?;
    let p = sandbox.resolve_existing(path)?;
    let label = sandbox.display_path(&p);
    if !FileKind::of(&p).is_tabular() {
        return Err(PhnError::Unsupported(format!(
            "'{label}': a partner roster must be a CSV or XLSX file with one organization per row"
        )));
    }
    let t = phn_local::table::read_table(&p, None)?;
    let norm = |s: &str| phn_core::text::normalize(s);
    // A list of people is refused (contact people in an organization list are fine).
    const PERSON_HEADERS: [&str; 14] = [
        "first name",
        "last name",
        "firstname",
        "lastname",
        "client",
        "client name",
        "patient",
        "patient name",
        "participant",
        "recipient",
        "dob",
        "date of birth",
        "birth date",
        "ssn",
    ];
    let bad: Vec<&String> = t
        .headers
        .iter()
        .filter(|h| {
            let n = norm(h);
            PERSON_HEADERS
                .iter()
                .any(|x| n == *x || n.starts_with(&format!("{x} ")))
        })
        .collect();
    let findings = phi::scan_table(&t);
    let bad_kinds: Vec<&str> = findings
        .iter()
        .filter(|f| {
            matches!(
                f.kind,
                Identifier::BirthDate
                    | Identifier::Ssn
                    | Identifier::MedicalRecordNumber
                    | Identifier::HealthPlanNumber
                    | Identifier::AccountNumber
            )
        })
        .map(|f| f.kind.label())
        .collect();
    if !bad.is_empty() || !bad_kinds.is_empty() {
        return Err(PhnError::Privacy(format!(
            "'{label}' looks like a list of people (client or patient columns, birth dates, or \
             record numbers), not a list of partner organizations, so it was not read. List \
             organizations only: name, sector or type, and optionally town."
        )));
    }
    let find = |names: &[&str]| -> Option<usize> {
        t.headers
            .iter()
            .position(|h| names.iter().any(|n| norm(h) == *n))
    };
    let org = find(&[
        "organization",
        "organisation",
        "agency",
        "partner",
        "name of organization",
        "organization name",
        "partner organization",
    ])
    .or_else(|| find(&["name"]));
    let sector = find(&[
        "sector",
        "type",
        "category",
        "organization type",
        "service type",
    ]);
    let (Some(org), Some(sector)) = (org, sector) else {
        return Err(PhnError::InvalidInput(format!(
            "'{label}' needs an organization name column and a sector or type column. Columns \
             found: {}",
            t.headers
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )));
    };
    let rows: Vec<(String, String)> = t
        .rows
        .iter()
        .map(|r| {
            (
                r.get(org).cloned().unwrap_or_default().trim().to_string(),
                r.get(sector)
                    .cloned()
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
            )
        })
        .filter(|(o, _)| !o.is_empty())
        .collect();
    if rows.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "'{label}' has no organizations"
        )));
    }
    Ok(Roster { label, rows })
}

/// Sector ids whose keywords appear (as words) in the text.
fn sectors_for(text: &str, sectors: &[SectorDef]) -> Vec<usize> {
    let t = format!(" {} ", phn_core::text::normalize(text));
    sectors
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            s.keywords
                .iter()
                .any(|k| t.contains(&format!(" {} ", phn_core::text::normalize(k))))
        })
        .map(|(i, _)| i)
        .collect()
}

// ---------------------------------------------------------------- hospitals

fn hospital_rows(
    ctx: &Context,
    area: &Area,
    named: &[String],
) -> Result<(Vec<HospitalRow>, Source)> {
    let (text, entry, _origin) = ctx
        .snapshots
        .file_text(HOSPITALS_SNAPSHOT, HOSPITALS_FILE)?;
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    #[derive(Deserialize)]
    struct Row {
        name: String,
        town: String,
        county: String,
        #[serde(rename = "type")]
        kind: String,
        ownership: String,
    }
    let norm = |s: &str| phn_core::text::normalize(s);
    let county = area.county.clone().unwrap_or_default();
    let mut out: Vec<HospitalRow> = Vec::new();
    let mut named_hit: BTreeSet<usize> = BTreeSet::new();
    for r in rdr.deserialize::<Row>() {
        let r = r.map_err(|e| PhnError::Internal(format!("hospital snapshot: {e}")))?;
        let in_town = area.level == AreaLevel::Town && norm(&r.town) == norm(&area.name);
        let in_county = !county.is_empty() && norm(&r.county) == norm(&county);
        let by_name = named.iter().enumerate().find(|(_, n)| {
            let n = norm(n);
            !n.is_empty() && (norm(&r.name).contains(&n) || n.contains(&norm(&r.name)))
        });
        let relation = if by_name.is_some() {
            "named by you"
        } else if in_town {
            "in the town"
        } else if in_county {
            "in the county"
        } else {
            continue;
        };
        if let Some((i, _)) = by_name {
            named_hit.insert(i);
        }
        let nonprofit = r.ownership.to_lowercase().contains("non-profit");
        let acute = matches!(
            r.kind.as_str(),
            "Acute Care Hospitals" | "Critical Access Hospitals" | "Childrens"
        );
        out.push(HospitalRow {
            name: r.name,
            town: r.town,
            county: r.county,
            hospital_type: r.kind,
            ownership: r.ownership,
            relation: relation.into(),
            likely_files_community_benefits_report: nonprofit && acute,
        });
    }
    // Psychiatric and federal facilities in the county are listed after the general hospitals.
    let rank = |h: &HospitalRow| {
        (
            match h.relation.as_str() {
                "named by you" => 0,
                "in the town" => 1,
                _ => 2,
            },
            !h.likely_files_community_benefits_report,
            h.name.clone(),
        )
    };
    out.sort_by_key(rank);
    for (i, n) in named.iter().enumerate() {
        if !named_hit.contains(&i) && !n.trim().is_empty() {
            out.push(HospitalRow {
                name: clip(n, 120),
                town: String::new(),
                county: String::new(),
                hospital_type: "not found in the CMS list".into(),
                ownership: String::new(),
                relation: "named by you".into(),
                likely_files_community_benefits_report: false,
            });
        }
    }
    Ok((out, entry.to_source()))
}

// ---------------------------------------------------------------- assembly (sync part)

fn trim_markdown(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.trim().to_string();
    }
    // Cut at a blank line (paragraph or table boundary) before the limit.
    let mut out = String::new();
    for block in s.split("\n\n") {
        if out.chars().count() + block.chars().count() + 2 > max && !out.is_empty() {
            break;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(block);
    }
    if out.chars().count() > max {
        out = clip(&out, max);
    }
    out.trim().to_string()
}

fn modes_of(sources: &[Source]) -> Vec<String> {
    let mut m: Vec<String> = sources
        .iter()
        .filter(|s| s.mode != SourceMode::Computed)
        .map(|s| s.mode.label().to_string())
        .collect();
    m.sort();
    m.dedup();
    m
}

struct Collected {
    sources: Vec<Source>,
    caveats: Vec<String>,
}

impl Collected {
    fn take<T>(&mut self, env: &Envelope<T>) {
        for s in &env.sources {
            if !self
                .sources
                .iter()
                .any(|x| x.title == s.title && x.mode == s.mode)
            {
                self.sources.push(s.clone());
            }
        }
        for c in &env.caveats {
            if !self.caveats.contains(c) {
                self.caveats.push(c.clone());
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn section_from<T>(
    col: &mut Collected,
    id: &str,
    title: &str,
    tool: &str,
    wanted: bool,
    applicable: bool,
    part: Option<std::result::Result<Envelope<T>, String>>,
    headline: impl FnOnce(&T) -> Vec<String>,
) -> StatusSection {
    let mut s = StatusSection {
        id: id.into(),
        title: title.into(),
        tool: tool.into(),
        status: "not requested".into(),
        detail: None,
        headline: Vec::new(),
        excerpt: None,
        data_modes: Vec::new(),
    };
    if !wanted {
        return s;
    }
    if !applicable {
        s.status = "not for this level".into();
        s.detail = Some(format!(
            "{tool} works by town: name a town to include this section."
        ));
        return s;
    }
    match part {
        Some(Ok(env)) => {
            s.status = "included".into();
            s.headline = headline(&env.data);
            s.excerpt = Some(trim_markdown(&env.summary, EXCERPT_CHARS));
            s.data_modes = modes_of(&env.sources);
            col.take(&env);
        }
        Some(Err(e)) => {
            s.status = "unavailable".into();
            s.detail = Some(e.clone());
            col.caveats
                .push(format!("{title}: not included because {tool} failed: {e}"));
        }
        None => {
            s.status = "unavailable".into();
            s.detail = Some("the section was not run".into());
        }
    }
    s
}

fn fmt1(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.1}")).unwrap_or_else(|| "n/a".into())
}

/// Build the assessment from what `gather` returned and the local files.
pub fn assemble(
    ctx: &Context,
    args: CommunityAssessmentArgs,
    g: Gathered,
) -> Result<Envelope<CommunityAssessmentResult>> {
    let c: ChaContent = content::load(CHA_FRAMEWORK)?;
    let Gathered {
        area,
        wanted,
        profile,
        language,
        climate,
        water,
    } = g;
    let want = |k: &str| wanted.contains(k);
    let town_level = area.level == AreaLevel::Town;
    let mut col = Collected {
        sources: Vec::new(),
        caveats: Vec::new(),
    };
    let mut caveats: Vec<String> = Vec::new();

    // ---- Community status assessment.
    let mut sections: Vec<StatusSection> = Vec::new();
    let mut profile_indicators: Vec<(String, String, Option<f64>, Option<f64>)> = Vec::new();
    let mut places_rows: Vec<(String, f64, Option<f64>, Option<String>)> = Vec::new();
    if let Some(Ok(env)) = &profile {
        for i in env
            .data
            .indicators
            .iter()
            .filter(|i| !i.id.ends_with("_count") && i.section != "Language counts")
        {
            profile_indicators.push((i.label.clone(), i.id.clone(), i.value, i.massachusetts));
        }
        if let Some(ch) = &env.data.chronic_disease {
            for r in &ch.rows {
                places_rows.push((
                    r.measure.clone(),
                    r.value,
                    r.massachusetts,
                    r.vs_massachusetts.clone(),
                ));
            }
        }
    }
    sections.push(section_from(
        &mut col,
        "profile",
        "Community profile, chronic disease (CDC PLACES), and Social Vulnerability Index",
        "build_community_health_profile",
        want("profile"),
        true,
        profile,
        |p| {
            let get = |id: &str| {
                p.indicators
                    .iter()
                    .find(|i| i.id == id)
                    .and_then(|i| i.value)
            };
            let mut h = vec![
                format!(
                    "Population {}; median age {}; age 65 and older {}%",
                    get("pop_total")
                        .map(|v| md::num(v, 0))
                        .unwrap_or_else(|| "n/a".into()),
                    fmt1(get("median_age")),
                    fmt1(get("age_65plus_pct"))
                ),
                format!(
                    "Income below the poverty level {}%; no health insurance {}%; limited English {}%",
                    fmt1(get("poverty_pct")),
                    fmt1(get("uninsured_pct")),
                    fmt1(get("lep_pct"))
                ),
                format!(
                    "ACS {} ({} figures flagged as unreliable, coefficient of variation above 30%)",
                    p.acs_period, p.unreliable_count
                ),
            ];
            if let Some(s) = &p.svi {
                h.push(format!(
                    "Social Vulnerability Index ({}): {} tracts; {} at or above the 75th percentile in Massachusetts",
                    s.release, s.tracts, s.tracts_high_vulnerability
                ));
            }
            h
        },
    ));
    sections.push(section_from(
        &mut col,
        "language",
        "Language access",
        "language_access_profile",
        want("language"),
        true,
        language,
        |l| {
            let mut h = vec![format!(
                "{}% of residents age 5 and older speak English less than very well",
                fmt1(l.lep_percent)
            )];
            let top: Vec<String> = l
                .groups
                .iter()
                .filter(|g| g.lep.value > 0.0)
                .take(3)
                .map(|g| {
                    format!(
                        "{} ({} with limited English)",
                        g.language_group,
                        md::num(g.lep.value, 0)
                    )
                })
                .collect();
            if !top.is_empty() {
                h.push(format!(
                    "Largest language groups with limited English: {}",
                    top.join("; ")
                ));
            }
            h
        },
    ));
    // Synchronous data tools.
    let svi: Option<std::result::Result<Envelope<SviResult>, String>> =
        want("social_vulnerability").then(|| {
            get_social_vulnerability_index(
                ctx,
                SviArgs {
                    area: Some(area.name.clone()),
                    top: Some(5),
                    ..Default::default()
                },
            )
            .map_err(|e| e.to_string())
        });
    sections.push(section_from(
        &mut col,
        "social_vulnerability",
        "Social Vulnerability Index (CDC/ATSDR)",
        "get_social_vulnerability_index",
        want("social_vulnerability"),
        true,
        svi,
        |s| {
            vec![format!(
                "{} of {} tracts ranked; median rank {}; tracts by quartile (least to most vulnerable): {:?}",
                s.tracts_ranked,
                s.tracts_total,
                s.median_rank
                    .map(|m| format!("{m:.2}"))
                    .unwrap_or_else(|| "n/a".into()),
                s.quartile_counts
            )]
        },
    ));
    let ej = want("environmental_justice").then(|| {
        environmental_justice_screen(
            ctx,
            EjScreenArgs {
                area: Some(area.name.clone()),
                map: Some(false),
                table: Some(false),
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())
    });
    sections.push(section_from(
        &mut col,
        "environmental_justice",
        "Environmental justice populations",
        "environmental_justice_screen",
        want("environmental_justice"),
        true,
        ej,
        |_| Vec::new(),
    ));
    sections.push(section_from(
        &mut col,
        "climate",
        "Climate and health vulnerability",
        "climate_health_vulnerability_profile",
        want("climate"),
        true,
        climate,
        |r| {
            r.most_important
                .iter()
                .take(3)
                .map(|v| {
                    format!(
                        "{}: {} (Massachusetts {})",
                        v.label, v.town, v.massachusetts
                    )
                })
                .collect()
        },
    ));
    let food = want("food_access").then(|| {
        food_access_mapper(
            ctx,
            FoodAccessArgs {
                area: Some(area.name.clone()),
                map: Some(false),
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())
    });
    sections.push(section_from(
        &mut col,
        "food_access",
        "Food access",
        "food_access_mapper",
        want("food_access"),
        true,
        food,
        |_| Vec::new(),
    ));
    sections.push(section_from(
        &mut col,
        "drinking_water",
        "Drinking water quality",
        "drinking_water_quality_lookup",
        want("drinking_water"),
        town_level,
        water,
        |_| Vec::new(),
    ));
    let fluor = (want("fluoridation") && town_level).then(|| {
        community_water_fluoridation_status(
            ctx,
            FluoridationArgs {
                town: Some(area.name.clone()),
                brief: Some(false),
            },
        )
        .map_err(|e| e.to_string())
    });
    sections.push(section_from(
        &mut col,
        "fluoridation",
        "Community water fluoridation",
        "community_water_fluoridation_status",
        want("fluoridation"),
        town_level,
        fluor,
        |_| Vec::new(),
    ));
    let gaps = vec![
        "Health disparities by race, ethnicity, language, or income need your own data: use \
         measure_health_disparities (EQ-04) with a table of counts from DPH or your agency."
            .to_string(),
        "Births, deaths, and other vital statistics are not included (no machine-readable \
         town-level source was available); use DPH's published reports."
            .to_string(),
        "Health-related social needs among your clients are covered by screen_health_related_social_needs \
         (EQ-05) from your own screening file."
            .to_string(),
    ];

    // ---- Framework.
    let fw = &c.framework;
    let cite = |src: &str, section: &str| {
        format!(
            "{}, {}",
            c.sources.get(src).map(|s| s.title.as_str()).unwrap_or(src),
            section
        )
    };
    let framework = FrameworkOut {
        name: fw.name.clone(),
        owner: fw.owner.clone(),
        released: fw.released.clone(),
        source_status: fw
            .source_status
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        phases: fw
            .phases
            .iter()
            .map(|p| PhaseOut {
                number: p.number,
                name: p.name.clone(),
                summary: p.summary.split_whitespace().collect::<Vec<_>>().join(" "),
                activities: content::cite_all(&p.activities, &c.sources),
                confidence: p.confidence,
            })
            .collect(),
        assessments: fw
            .assessments
            .iter()
            .map(|a| AssessmentOut {
                name: a.name.clone(),
                data: a.data.clone(),
                purpose: a.purpose.clone(),
                server_tools: a.server_tools.clone(),
                confidence: a.confidence,
                citation: cite(&a.source, &a.section),
            })
            .collect(),
        equity_note: content::cite_all(std::slice::from_ref(&fw.equity_note), &c.sources).remove(0),
    };

    // ---- Partner assessment.
    let mut identifiers: Vec<String> = Vec::new();
    let mut local_sources: Vec<Source> = Vec::new();
    let mut checklist: Vec<SectorRow> = c
        .partner_sectors
        .iter()
        .map(|s| SectorRow {
            sector: s.label.clone(),
            why: s.why.clone(),
            partners: None,
        })
        .collect();
    let mut roster_summary: Option<RosterSummary> = None;
    let mut roster_orgs: Vec<Vec<String>> = vec![Vec::new(); c.partner_sectors.len()];
    if let Some(p) = &args.partners_path {
        let r = load_roster(ctx, p)?;
        let mut counts = vec![0usize; c.partner_sectors.len()];
        let mut unmatched = 0usize;
        for (org, sector_text) in &r.rows {
            let hits = sectors_for(&format!("{sector_text} {org}"), &c.partner_sectors);
            if hits.is_empty() {
                unmatched += 1;
            }
            for h in hits {
                counts[h] += 1;
                roster_orgs[h].push(org.clone());
            }
        }
        for (row, n) in checklist.iter_mut().zip(&counts) {
            row.partners = Some(*n);
        }
        let missing: Vec<String> = checklist
            .iter()
            .filter(|r| r.partners == Some(0))
            .map(|r| r.sector.clone())
            .collect();
        roster_summary = Some(RosterSummary {
            source_file: r.label.clone(),
            organizations: r.rows.len(),
            unmatched,
            sectors_covered: checklist.len() - missing.len(),
            sectors_total: checklist.len(),
            missing_sectors: missing,
        });
        local_sources.push(Source::local_file(r.label));
    }
    let partner = PartnerAssessment {
        facts: content::cite_all(&fw.partner_assessment_facts, &c.sources),
        sector_checklist: checklist,
        roster: roster_summary,
        steps: vec![
            "List the organizations that touch the community's health, by sector, and mark who is missing (use find_community_resources for a directory of services, and save your roster as a CSV with an organization name and a sector).".into(),
            "Ask each partner to assess its own capacity and priorities (MAPP 2.0's Community Partner Assessment uses meetings and a survey; get NACCHO's current tool).".into(),
            "Record which populations each partner serves and where the gaps are.".into(),
        ],
    };

    // ---- Context assessment.
    let context = ContextAssessment {
        purpose: fw
            .assessments
            .iter()
            .find(|a| a.id == "context")
            .map(|a| format!("{} ({}): {}", a.name, a.data.to_lowercase(), a.purpose))
            .unwrap_or_default(),
        prompts: c.context_prompts.clone(),
        how_to: content::cite_all(&c.context_how_to, &c.sources),
    };

    // ---- Hospitals and CHNAs.
    let (hospitals, hosp_source) = hospital_rows(ctx, &area, &args.hospitals)?;
    col.sources.push(hosp_source);
    let (reports, chna_sources, chna_identifiers) = if args.chna_paths.is_empty() {
        (Vec::new(), Vec::new(), Vec::new())
    } else {
        read_chna_reports(ctx, &args.chna_paths, &c.chna.theme_vocabulary)?
    };
    local_sources.extend(chna_sources);
    identifiers.extend(chna_identifiers);
    let mut theme_counts: BTreeMap<String, usize> = BTreeMap::new();
    for r in &reports {
        for t in &r.themes {
            *theme_counts.entry(t.clone()).or_default() += 1;
        }
    }
    let mut common: Vec<ThemeRow> = theme_counts
        .into_iter()
        .map(|(theme, reports)| ThemeRow { theme, reports })
        .collect();
    common.sort_by(|a, b| b.reports.cmp(&a.reports).then(a.theme.cmp(&b.theme)));
    let statewide = vec![
        "Chronic disease with a focus on cancer, heart disease, and diabetes".to_string(),
        "Housing stability and homelessness".to_string(),
        "Mental illness and mental health".to_string(),
        "Substance use disorders".to_string(),
    ];
    let hospital_section = HospitalSection {
        hospitals,
        federal_requirements: content::cite_all(&c.chna.federal, &c.sources),
        massachusetts_guidance: content::cite_all(&c.chna.massachusetts, &c.sources),
        required_elements: c.chna.elements.clone(),
        directory: c
            .chna
            .directory
            .iter()
            .map(|d| DirectoryLink {
                title: d.title.clone(),
                url: d.url.clone(),
                what: d.what.clone(),
                confidence: d.confidence,
                note: d.note.clone(),
            })
            .collect(),
        how_to_collect: c.chna.how_to_collect.clone(),
        reports_read: reports,
        common_themes: common,
        statewide_focus_issues: statewide,
    };

    // ---- Next steps.
    let mut next_steps = vec![
        "Phase 1 (build the foundation): decide who is at the table and who is missing (the partner checklist), and agree a vision; MAPP 2.0's stakeholder and power analysis and Starting Point Assessment are NACCHO's tools for this.".to_string(),
        "Phase 2 (tell the community story): finish the three assessments. Status: the sections above. Context: hold focus groups and interviews, then run analyze_qualitative_data (EQ-10) on the transcripts. Partner: complete the sector checklist.".to_string(),
        "Collect nearby hospitals' CHNAs (the list and links above), save them in /data/chna, and run this tool again with chna_paths.".to_string(),
        "Look across everything for themes and list the candidate issues; then rank them with prioritize_health_issues (EQ-09: Hanlon method, PEARL, strategy grid, nominal group).".to_string(),
        "Phase 3 (continuously improve): plan, carry out, and evaluate the community health improvement plan; the program planning tools come in a later phase.".to_string(),
    ];
    if hospital_section.reports_read.is_empty() {
        next_steps.insert(
            0,
            "No CHNA files were read yet: hospitals' findings are not part of this draft.".into(),
        );
    }

    // ---- Document.
    let mut result = CommunityAssessmentResult {
        area: AreaOut {
            name: area.name.clone(),
            level: area.level,
            county: area.county.clone(),
        },
        framework,
        status_assessment: StatusAssessment { sections, gaps },
        context_assessment: context,
        partner_assessment: partner,
        hospital_chnas: hospital_section,
        next_steps,
        not_verified: c.not_verified.clone(),
        document: None,
    };
    let mut privacy = Privacy {
        row_level_data: false,
        ..Privacy::none()
    };
    if args.write_document.unwrap_or(true) {
        match ctx.sandbox() {
            Ok(_) => {
                let text = document_markdown(
                    &result,
                    &c,
                    &profile_indicators,
                    &places_rows,
                    &roster_orgs,
                    &col,
                );
                let doc = templates::document(
                    ctx,
                    &text,
                    "en-US",
                    &format!("Community health assessment draft: {}", area.name),
                );
                let format = args.format.unwrap_or_default();
                let (dir, shown) = templates::output_folder(ctx, "community_health_assessment")?;
                let file = templates::write_in(
                    ctx,
                    &dir,
                    &format!("community_health_assessment_draft.{}", format.ext()),
                    &format.render(&doc)?,
                )?;
                privacy.files_written.push(file.clone());
                privacy.notes.push(format!(
                    "The draft report sections were written to {shown}. They hold public data, \
                     counts, and (if you gave a partner roster) the names of partner \
                     organizations; contact people were not read."
                ));
                result.document = Some(file);
            }
            Err(e) => caveats.push(format!("The draft report was not written: {e}")),
        }
    }
    privacy.identifiers_detected = {
        identifiers.sort();
        identifiers.dedup();
        identifiers
    };
    if !args.chna_paths.is_empty() {
        privacy.notes.push(
            "CHNA reports are public documents. Only priority lines (identifiers removed) and page \
             numbers were taken from them."
                .into(),
        );
    }
    if args.partners_path.is_some() {
        privacy.notes.push(
            "Only organization names and sectors were read from your partner roster; any contact \
             people, phone numbers, and emails in it were not read or returned."
                .into(),
        );
    }

    // ---- Caveats and sources.
    caveats.push(
        "MAPP 2.0 was checked against journal articles, not NACCHO's own materials (naccho.org \
         could not be opened): the three phases and three assessments are described in those \
         articles, but the detail of each phase and NACCHO's tools should be checked against \
         NACCHO's handbook. The mapping of this server's tools to the assessments is this \
         project's own."
            .into(),
    );
    caveats.push(
        "This is a draft built from public secondary data. It does not replace the community \
         partner and context assessments, which need your partners and your residents."
            .into(),
    );
    let failed = result
        .status_assessment
        .sections
        .iter()
        .filter(|s| s.status == "unavailable")
        .count();
    if failed > 0 {
        caveats.push(format!(
            "{failed} data section(s) could not be built (see each section's reason); the rest \
             are included."
        ));
    }
    if !col.caveats.is_empty() {
        caveats.push(format!(
            "The data tools reported {} caveats (small numbers, data vintages, estimates); they \
             are listed in the draft report and in each tool's own output.",
            col.caveats.len()
        ));
    }
    if !result.hospital_chnas.reports_read.is_empty() {
        caveats.push(
            "Priority lines were extracted from the CHNA files by pattern and can be incomplete \
             or include nearby headings; the theme words are this project's own list. Check each \
             against the report before quoting it."
                .into(),
        );
    }
    caveats.extend(content::review_caveats(&[CHA_FRAMEWORK]));
    let mut sources = content::sources_for(
        [
            "mapp-frontiers-2025",
            "mapp-inquiry-2025",
            "cfr-501r3",
            "ag-guidelines",
            "ag-cb-page",
        ],
        &c.sources,
    );
    sources.extend(col.sources.iter().cloned());
    sources.extend(local_sources);
    let summary = summary_text(&result);
    Ok(Envelope::new(summary, result)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy)
        .decision_support())
}

fn summary_text(r: &CommunityAssessmentResult) -> String {
    let mut s = format!(
        "**Community health assessment: {}** (framework: {}, released {})\n\n",
        r.area.name, r.framework.name, r.framework.released
    );
    s.push_str("**MAPP 2.0 phases**\n\n");
    for p in &r.framework.phases {
        s.push_str(&format!(
            "{}. {}{}\n",
            p.number,
            p.name,
            if p.confidence == Confidence::Low {
                " (low confidence: not verified in detail)"
            } else {
                ""
            }
        ));
    }
    s.push_str("\n**Community status assessment (quantitative data)**\n\n");
    let rows: Vec<Vec<String>> = r
        .status_assessment
        .sections
        .iter()
        .map(|x| {
            vec![
                x.title.clone(),
                x.status.clone(),
                if x.headline.is_empty() {
                    x.detail.clone().unwrap_or_default()
                } else {
                    x.headline.join(" | ")
                },
            ]
        })
        .collect();
    s.push_str(&md::table(&["Section", "Status", "Headline"], &rows));
    s.push_str("\n**Community partner assessment**\n\n");
    match &r.partner_assessment.roster {
        Some(ro) => s.push_str(&format!(
            "{} organizations in your roster cover {} of {} sectors; no organization yet in: {}.\n",
            ro.organizations,
            ro.sectors_covered,
            ro.sectors_total,
            if ro.missing_sectors.is_empty() {
                "none".to_string()
            } else {
                ro.missing_sectors.join("; ")
            }
        )),
        None => s.push_str(&format!(
            "{} sectors to look for (see the checklist); give partners_path to count your roster.\n",
            r.partner_assessment.sector_checklist.len()
        )),
    }
    s.push_str("\n**Community context assessment (qualitative data)**\n\n");
    s.push_str(&format!(
        "{} Use analyze_qualitative_data on focus group and interview transcripts.\n",
        r.context_assessment.purpose
    ));
    s.push_str("\n**Hospital CHNAs**\n\n");
    let hs: Vec<String> = r
        .hospital_chnas
        .hospitals
        .iter()
        .take(12)
        .map(|h| {
            format!(
                "{} ({}, {}{})",
                h.name,
                if h.town.is_empty() { "?" } else { &h.town },
                h.relation,
                if h.likely_files_community_benefits_report {
                    "; files a community benefits report"
                } else {
                    ""
                }
            )
        })
        .collect();
    s.push_str(&format!(
        "{} hospitals in or near the area: {}.\n",
        r.hospital_chnas.hospitals.len(),
        if hs.is_empty() {
            "none listed".to_string()
        } else {
            hs.join("; ")
        }
    ));
    if r.hospital_chnas.reports_read.is_empty() {
        s.push_str(
            "No CHNA files were read. Find them through the Attorney General's community benefits \
             reports (links in the result), save them in /data, and pass chna_paths.\n",
        );
    } else {
        for rep in &r.hospital_chnas.reports_read {
            s.push_str(&format!(
                "- {} ({}{}): {} priority lines; themes: {}\n",
                rep.id,
                rep.file,
                rep.year_seen.map(|y| format!(", {y}")).unwrap_or_default(),
                rep.priorities.len(),
                if rep.themes.is_empty() {
                    "none recognized".to_string()
                } else {
                    rep.themes.join(", ")
                }
            ));
        }
        if !r.hospital_chnas.common_themes.is_empty() {
            s.push_str(&format!(
                "Themes across the reports: {}.\n",
                r.hospital_chnas
                    .common_themes
                    .iter()
                    .map(|t| format!("{} ({})", t.theme, t.reports))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    s.push_str("\n**Next steps**\n\n");
    for (i, n) in r.next_steps.iter().enumerate() {
        s.push_str(&format!("{}. {}\n", i + 1, n));
    }
    if let Some(d) = &r.document {
        s.push_str(&format!("\nDraft report sections written to {d}\n"));
    }
    s
}

#[allow(clippy::too_many_arguments)]
fn document_markdown(
    r: &CommunityAssessmentResult,
    c: &ChaContent,
    indicators: &[(String, String, Option<f64>, Option<f64>)],
    places: &[(String, f64, Option<f64>, Option<String>)],
    roster_orgs: &[Vec<String>],
    col: &Collected,
) -> String {
    let mut m = String::new();
    let outline = |id: &str| -> (String, String) {
        c.report_outline
            .iter()
            .find(|o| o.id == id)
            .map(|o| (o.title.clone(), o.guidance.clone()))
            .unwrap_or_default()
    };
    m.push_str(&format!(
        "# Community health assessment draft: {}\n\n",
        r.area.name
    ));
    m.push_str(
        "_Draft sections built by the Public Health Nursing MCP Server from public data and your \
         files. Review every figure against its source before sharing. Text in [brackets] is for \
         you to complete._\n\n",
    );
    let (t, g) = outline("purpose");
    m.push_str(&format!("## 1. {t}\n\n_{g}_\n\n"));
    m.push_str(&format!(
        "This draft follows {}, released by {} in {}. {}\n\n",
        r.framework.name, r.framework.owner, r.framework.released, r.framework.source_status
    ));
    for p in &r.framework.phases {
        m.push_str(&format!(
            "- **Phase {}: {}.** {}\n",
            p.number, p.name, p.summary
        ));
    }
    m.push_str(
        "\n[Lead agency, partners at the table, dates, and how residents were involved.]\n\n",
    );
    let (t, g) = outline("community");
    m.push_str(&format!("## 2. {t}\n\n_{g}_\n\n"));
    if indicators.is_empty() {
        m.push_str("[The community profile was not included.]\n\n");
    } else {
        let rows: Vec<Vec<String>> = indicators
            .iter()
            .map(|(label, _, v, ma)| {
                vec![
                    label.clone(),
                    v.map(|x| format!("{x:.1}")).unwrap_or_else(|| "n/a".into()),
                    ma.map(|x| format!("{x:.1}"))
                        .unwrap_or_else(|| "n/a".into()),
                ]
            })
            .collect();
        m.push_str(&md::table(
            &["Indicator", &r.area.name, "Massachusetts"],
            &rows,
        ));
        m.push('\n');
    }
    let (t, g) = outline("status");
    m.push_str(&format!("## 3. {t}\n\n_{g}_\n\n"));
    if !places.is_empty() {
        m.push_str("### Chronic disease and risk factors (CDC PLACES)\n\n");
        let rows: Vec<Vec<String>> = places
            .iter()
            .map(|(label, v, ma, vs)| {
                vec![
                    label.clone(),
                    format!("{v:.1}"),
                    ma.map(|x| format!("{x:.1}"))
                        .unwrap_or_else(|| "n/a".into()),
                    vs.clone().unwrap_or_default(),
                ]
            })
            .collect();
        m.push_str(&md::table(
            &[
                "Measure (adults, %)",
                &r.area.name,
                "Massachusetts",
                "Compared with Massachusetts",
            ],
            &rows,
        ));
        m.push('\n');
    }
    for s in &r.status_assessment.sections {
        m.push_str(&format!("### {}\n\n", s.title));
        match s.status.as_str() {
            "included" => {
                for h in &s.headline {
                    m.push_str(&format!("- {h}\n"));
                }
                if let Some(e) = &s.excerpt {
                    m.push_str(&format!("\n{e}\n\n"));
                }
                m.push_str(&format!(
                    "_Data: {} ({})._\n\n",
                    s.tool,
                    if s.data_modes.is_empty() {
                        "computed".to_string()
                    } else {
                        s.data_modes.join(", ")
                    }
                ));
            }
            other => m.push_str(&format!(
                "Not included ({other}){}\n\n",
                s.detail
                    .as_ref()
                    .map(|d| format!(": {d}"))
                    .unwrap_or_default()
            )),
        }
    }
    m.push_str("### Gaps\n\n");
    for g in &r.status_assessment.gaps {
        m.push_str(&format!("- {g}\n"));
    }
    m.push('\n');
    let (t, g) = outline("context");
    m.push_str(&format!("## 4. {t}\n\n_{g}_\n\n"));
    m.push_str(&format!("{}\n\n", r.context_assessment.purpose));
    m.push_str(
        "Questions for focus groups and key informant interviews (this project's drafts):\n\n",
    );
    for p in &r.context_assessment.prompts {
        m.push_str(&format!("- {p}\n"));
    }
    m.push_str("\n[Themes from focus groups and interviews: use the theme counts from analyze_qualitative_data. Strengths and assets: ______. Forces of change: ______.]\n\n");
    let (t, g) = outline("partners");
    m.push_str(&format!("## 5. {t}\n\n_{g}_\n\n"));
    for f in &r.partner_assessment.facts {
        m.push_str(&format!("- {} _({})_\n", f.text, f.citation));
    }
    m.push('\n');
    let rows: Vec<Vec<String>> = r
        .partner_assessment
        .sector_checklist
        .iter()
        .enumerate()
        .map(|(i, s)| {
            vec![
                s.sector.clone(),
                s.partners
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "[ ]".into()),
                roster_orgs.get(i).map(|v| v.join("; ")).unwrap_or_default(),
                s.why.clone(),
            ]
        })
        .collect();
    m.push_str(&md::table(
        &["Sector", "Organizations", "Names", "Why it matters"],
        &rows,
    ));
    m.push('\n');
    let (t, g) = outline("hospitals");
    m.push_str(&format!("## 6. {t}\n\n_{g}_\n\n"));
    for x in &r.hospital_chnas.federal_requirements {
        m.push_str(&format!("- {} _({})_\n", x.text, x.citation));
    }
    for x in &r.hospital_chnas.massachusetts_guidance {
        m.push_str(&format!("- {} _({})_\n", x.text, x.citation));
    }
    m.push_str("\n### Hospitals in or near the area (CMS)\n\n");
    let rows: Vec<Vec<String>> = r
        .hospital_chnas
        .hospitals
        .iter()
        .map(|h| {
            vec![
                h.name.clone(),
                h.town.clone(),
                h.hospital_type.clone(),
                h.ownership.clone(),
                h.relation.clone(),
            ]
        })
        .collect();
    if !rows.is_empty() {
        m.push_str(&md::table(
            &["Hospital", "Town", "Type", "Ownership", "Relation"],
            &rows,
        ));
        m.push('\n');
    }
    m.push_str("### Where to find CHNAs\n\n");
    for d in &r.hospital_chnas.directory {
        m.push_str(&format!(
            "- {} ({}): {} {}\n",
            d.title, d.url, d.what, d.note
        ));
    }
    m.push('\n');
    if r.hospital_chnas.reports_read.is_empty() {
        m.push_str("[No CHNA files were read. Save hospitals' CHNAs in /data/chna and run the tool again with chna_paths.]\n\n");
    } else {
        m.push_str("### Priorities in the CHNAs read\n\n");
        for rep in &r.hospital_chnas.reports_read {
            m.push_str(&format!(
                "**{} ({}{}), {} pages**\n\n",
                rep.id,
                rep.file,
                rep.year_seen.map(|y| format!(", {y}")).unwrap_or_default(),
                rep.pages
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "?".into())
            ));
            if rep.priorities.is_empty() {
                m.push_str(&format!("{}\n\n", rep.note));
            } else {
                for p in &rep.priorities {
                    m.push_str(&format!("- {} _({})_\n", p.text, p.location));
                }
                m.push_str(&format!("\n_{}_\n\n", rep.note));
            }
        }
        if !r.hospital_chnas.common_themes.is_empty() {
            let rows: Vec<Vec<String>> = r
                .hospital_chnas
                .common_themes
                .iter()
                .map(|t| vec![t.theme.clone(), t.reports.to_string()])
                .collect();
            m.push_str(&md::table(&["Theme", "Reports that mention it"], &rows));
            m.push('\n');
        }
    }
    let (t, g) = outline("synthesis");
    m.push_str(&format!("## 7. {t}\n\n_{g}_\n\n"));
    m.push_str("[Themes that appear in the status data, the community context, the partner assessment, and the hospital CHNAs. Candidate issues to rank: ______.]\n\n");
    let (t, g) = outline("limits");
    m.push_str(&format!("## 8. {t}\n\n_{g}_\n\n"));
    for n in &r.not_verified {
        m.push_str(&format!("- Not verified when this tool was built: {n}\n"));
    }
    for cv in &col.caveats {
        m.push_str(&format!("- {cv}\n"));
    }
    m.push_str("\n## Sources\n\n");
    for s in &col.sources {
        m.push_str(&format!(
            "- {} ({}){} [{}]\n",
            s.title,
            s.publisher,
            s.url.as_ref().map(|u| format!(", {u}")).unwrap_or_default(),
            s.mode.label()
        ));
    }
    for k in [
        "mapp-frontiers-2025",
        "mapp-inquiry-2025",
        "cfr-501r3",
        "ag-guidelines",
        "ag-cb-page",
    ] {
        if let Some(s) = c.sources.get(k) {
            m.push_str(&format!("- {} ({}), {}\n", s.title, s.publisher, s.url));
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab() -> Vec<ThemeDef> {
        let c: ChaContent = content::load(CHA_FRAMEWORK).unwrap();
        c.chna.theme_vocabulary
    }

    fn section(heading: &str, page: Option<u32>, text: &str) -> Section {
        Section {
            heading: heading.into(),
            level: 1,
            page,
            text: text.into(),
        }
    }

    #[test]
    fn content_loads() {
        let c: ChaContent = content::load(CHA_FRAMEWORK).unwrap();
        assert_eq!(c.framework.phases.len(), 3);
        assert_eq!(c.framework.assessments.len(), 3);
        assert!(c.partner_sectors.len() >= 10);
        assert!(c.chna.theme_vocabulary.len() >= 10);
        assert!(
            c.chna
                .directory
                .iter()
                .all(|d| d.url.starts_with("https://"))
        );
    }

    #[test]
    fn finds_bulleted_and_inline_priority_lists_with_pages() {
        let secs = vec![
            section(
                "",
                Some(3),
                "Executive Summary\nThe significant health needs identified were:\n• Mental health and substance use\n• Housing stability\n• Access to primary care\n\nThe assessment used surveys and focus groups.\n",
            ),
            section(
                "",
                Some(9),
                "Our top priorities: chronic disease; food insecurity; transportation.\n",
            ),
        ];
        let (found, anchors) = extract_priorities(&secs, &vocab());
        assert_eq!(anchors, 2);
        let texts: Vec<&str> = found.iter().map(|p| p.text.as_str()).collect();
        assert!(
            texts.contains(&"Mental health and substance use"),
            "{texts:?}"
        );
        assert!(texts.contains(&"Housing stability"));
        assert!(texts.contains(&"chronic disease"));
        let mh = found
            .iter()
            .find(|p| p.text.starts_with("Mental health"))
            .unwrap();
        assert_eq!(mh.location, "page 3");
        assert!(mh.themes.contains(&"Mental health".to_string()));
        assert!(mh.themes.contains(&"Substance use".to_string()));
        let cd = found.iter().find(|p| p.text == "chronic disease").unwrap();
        assert_eq!(cd.location, "page 9");
    }

    #[test]
    fn numbered_lists_and_plain_heading_lines_are_found_and_long_paragraphs_stop_the_list() {
        let secs = vec![section(
            "Priorities",
            None,
            "Community Health Priorities\n1. Behavioral health\n2) Chronic conditions\n3. Social determinants of health\nThis paragraph describes how each of the priorities above was chosen by the steering committee after three rounds of discussion and voting among members.\n",
        )];
        let (found, anchors) = extract_priorities(&secs, &vocab());
        assert_eq!(anchors, 1);
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(found[0].location, "section \"Priorities\"");
        let secs = vec![section(
            "",
            Some(2),
            "Priority Areas\nHousing\nFood access\nTransportation\n",
        )];
        let (found, _) = extract_priorities(&secs, &vocab());
        assert_eq!(found.len(), 3, "{found:?}");
    }

    #[test]
    fn bullets_on_their_own_line_as_in_pdf_text_are_read() {
        let secs = vec![section(
            "Page 2",
            Some(2),
            "Significant health needs\n•\nMental health and substance use\n•\nHousing stability\nPage 2 of 3",
        )];
        let (found, anchors) = extract_priorities(&secs, &vocab());
        assert_eq!(anchors, 1);
        let texts: Vec<&str> = found.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["Mental health and substance use", "Housing stability"]
        );
        assert_eq!(found[0].location, "page 2");
    }

    #[test]
    fn identifiers_in_priority_lines_are_dropped() {
        let secs = vec![section(
            "",
            Some(1),
            "Priority contacts:\n• Call 413-555-0100 for housing\n• Housing stability\n",
        )];
        let (found, _) = extract_priorities(&secs, &vocab());
        let all: String = found
            .iter()
            .map(|p| p.text.clone())
            .collect::<Vec<_>>()
            .join("|");
        assert!(!all.contains("555"), "{all}");
        assert!(all.contains("Housing stability"));
    }

    #[test]
    fn long_lines_are_clipped_by_characters() {
        let long = "é".repeat(400);
        let c = clip(&long, MAX_PRIORITY_CHARS);
        assert_eq!(c.chars().count(), MAX_PRIORITY_CHARS + 1);
        assert!(c.ends_with('…'));
    }

    #[test]
    fn client_records_are_refused_without_echo() {
        let text = "Client: Rosa. DOB: 04/17/1958. MRN 00451234. SSN 921-55-0193.";
        let e = refuse_if_client_record(text).unwrap_err().to_string();
        assert!(e.contains("client record"), "{e}");
        assert!(!e.contains("1958") && !e.contains("0193"));
        // A public report with a phone number is fine.
        assert!(
            refuse_if_client_record("Call the community benefits office at 617-555-0100.").is_ok()
        );
    }

    #[test]
    fn markdown_is_trimmed_at_block_boundaries() {
        let md = format!(
            "{}\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n{}",
            "x".repeat(50),
            "y".repeat(2000)
        );
        let t = trim_markdown(&md, 120);
        assert!(t.contains("| 1 | 2 |"));
        assert!(!t.contains("yyyy"));
        assert!(t.chars().count() <= 120);
    }

    #[test]
    fn sections_are_validated() {
        let ok = wanted_sections(&CommunityAssessmentArgs {
            sections: vec!["Food access".into(), "profile".into()],
            ..Default::default()
        })
        .unwrap();
        assert_eq!(ok.len(), 2);
        let e = wanted_sections(&CommunityAssessmentArgs {
            sections: vec!["weather".into()],
            ..Default::default()
        })
        .unwrap_err()
        .to_string();
        assert!(e.contains("unknown section 'weather'"), "{e}");
    }

    #[test]
    fn sector_matching_uses_whole_words() {
        let c: ChaContent = content::load(CHA_FRAMEWORK).unwrap();
        let hit = |t: &str| -> Vec<String> {
            sectors_for(t, &c.partner_sectors)
                .into_iter()
                .map(|i| c.partner_sectors[i].id.clone())
                .collect()
        };
        assert!(hit("Food pantry").contains(&"social_services".to_string()));
        assert!(hit("Community Health Center").contains(&"health_care".to_string()));
        assert!(hit("Council on Aging").contains(&"aging".to_string()));
        assert!(hit("Zoning board").is_empty());
        // "schoolhouse" is not "school".
        assert!(!hit("Schoolhouse Gallery").contains(&"education".to_string()));
    }

    #[tokio::test]
    async fn builds_a_draft_for_a_town_offline_and_writes_a_document() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = community_health_assessment_builder(
            &ctx,
            CommunityAssessmentArgs {
                area: Some("Lowell".into()),
                sections: vec![
                    "profile".into(),
                    "social_vulnerability".into(),
                    "fluoridation".into(),
                ],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let r = &env.data;
        assert_eq!(r.area.name, "Lowell");
        assert_eq!(r.framework.phases.len(), 3);
        let by = |id: &str| {
            r.status_assessment
                .sections
                .iter()
                .find(|s| s.id == id)
                .unwrap()
        };
        assert_eq!(
            by("profile").status,
            "included",
            "{:?}",
            by("profile").detail
        );
        assert_eq!(by("social_vulnerability").status, "included");
        assert_eq!(by("language").status, "not requested");
        assert!(!by("profile").headline.is_empty());
        // Lowell General is in the CMS list.
        assert!(
            r.hospital_chnas
                .hospitals
                .iter()
                .any(|h| h.name.contains("Lowell General") && h.relation == "in the town"),
            "{:?}",
            r.hospital_chnas
                .hospitals
                .iter()
                .map(|h| h.name.clone())
                .collect::<Vec<_>>()
        );
        assert!(r.document.as_deref().is_some_and(|d| d.ends_with(".docx")));
        assert!(!env.privacy.row_level_data);
        let text =
            std::fs::read(dir.path().join("data").join(r.document.as_ref().unwrap())).unwrap();
        assert!(text.len() > 5000);
        assert!(env.to_markdown().contains("MAPP 2.0"));
    }

    #[tokio::test]
    async fn a_county_skips_town_only_sections_and_a_bad_area_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = community_health_assessment_builder(
            &ctx,
            CommunityAssessmentArgs {
                area: Some("Hampden County".into()),
                sections: vec!["fluoridation".into(), "drinking_water".into()],
                write_document: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        for s in &env.data.status_assessment.sections {
            if s.id == "fluoridation" || s.id == "drinking_water" {
                assert_eq!(s.status, "not for this level");
            }
        }
        assert!(
            env.data
                .hospital_chnas
                .hospitals
                .iter()
                .any(|h| h.relation == "in the county")
        );
        let e = community_health_assessment_builder(
            &ctx,
            CommunityAssessmentArgs {
                area: Some("Atlantis".into()),
                ..Default::default()
            },
        )
        .await
        .expect_err("error")
        .to_string();
        assert!(e.contains("not a Massachusetts"), "{e}");
        let e = community_health_assessment_builder(&ctx, CommunityAssessmentArgs::default())
            .await
            .expect_err("error")
            .to_string();
        assert!(
            e.contains("name a Massachusetts") || e.contains("not found"),
            "{e}"
        );
    }

    #[tokio::test]
    async fn reads_chna_files_and_a_partner_roster() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(data.join("chna")).unwrap();
        std::fs::write(
            data.join("chna/example_hospital_chna_2025.txt"),
            "Example General Hospital\nCommunity Health Needs Assessment 2025\n\nThe significant health needs identified were:\n- Mental health\n- Housing and homelessness\n- Chronic disease management\n",
        )
        .unwrap();
        std::fs::write(
            data.join("partners.csv"),
            "Organization,Type,Contact Person,Phone\nRiverbend Example Food Pantry,Food pantry,Pat Example,413-555-0101\nExample Health Center,Community health center,Sam Example,413-555-0102\n",
        )
        .unwrap();
        std::fs::write(
            data.join("clients.csv"),
            "Client Name,DOB,Type\nA B,2001-01-01,Food pantry\n",
        )
        .unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = community_health_assessment_builder(
            &ctx,
            CommunityAssessmentArgs {
                area: Some("Lowell".into()),
                sections: vec!["fluoridation".into()],
                chna_paths: vec!["chna".into()],
                partners_path: Some("partners.csv".into()),
                hospitals: vec!["Example General Hospital".into()],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let r = &env.data;
        assert_eq!(r.hospital_chnas.reports_read.len(), 1);
        let rep = &r.hospital_chnas.reports_read[0];
        assert_eq!(rep.year_seen, Some(2025));
        assert_eq!(rep.priorities.len(), 3, "{:?}", rep.priorities);
        assert!(
            r.hospital_chnas
                .common_themes
                .iter()
                .any(|t| t.theme == "Mental health" && t.reports == 1)
        );
        let ro = r.partner_assessment.roster.as_ref().unwrap();
        assert_eq!(ro.organizations, 2);
        assert!(ro.sectors_covered >= 2);
        // Contacts never appear.
        let all = serde_json::to_string(&env).unwrap() + &env.to_markdown();
        assert!(
            !all.contains("Pat Example") && !all.contains("0101") && !all.contains("Sam Example")
        );
        assert!(
            r.hospital_chnas
                .hospitals
                .iter()
                .any(|h| h.name == "Example General Hospital")
        );
        // A client list is refused.
        let e = community_health_assessment_builder(
            &ctx,
            CommunityAssessmentArgs {
                area: Some("Lowell".into()),
                sections: vec!["fluoridation".into()],
                partners_path: Some("clients.csv".into()),
                write_document: Some(false),
                ..Default::default()
            },
        )
        .await
        .expect_err("error")
        .to_string();
        assert!(e.contains("list of people"), "{e}");
        // A spreadsheet is not a CHNA.
        let e = community_health_assessment_builder(
            &ctx,
            CommunityAssessmentArgs {
                area: Some("Lowell".into()),
                sections: vec!["fluoridation".into()],
                chna_paths: vec!["partners.csv".into()],
                write_document: Some(false),
                ..Default::default()
            },
        )
        .await
        .expect_err("error")
        .to_string();
        assert!(e.contains("not a document"), "{e}");
    }
}
