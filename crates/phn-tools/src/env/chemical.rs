//! ENV-08 `chemical_exposure_reference`: look up a chemical by name, synonym,
//! or CAS number in the NIOSH Pocket Guide, the NIOSH IDLH table, the ATSDR
//! minimal risk level (MRL) table, and the ATSDR ToxFAQs index, with notes and
//! Massachusetts contacts for an exposure scenario (spill or release, indoor
//! air, contaminated site, drinking water, workplace).
//!
//! Every response says that occupational limits are not community limits and
//! that MRLs are screening levels. Contacts come from the reviewed content
//! file; nothing is invented here.

use std::collections::BTreeMap;

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::chemicals::{self, ChemicalReference, MrlRow, NpgEntry};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::content::{self, Cited, CitedOut, Confidence, DocSource, LinkDef};
use crate::md;

/// Content id of `content/env/chemical_exposure.yaml`.
pub const CHEMICAL_EXPOSURE: &str = "env-chemical-exposure";

/// Score a fuzzy match must reach to be used without asking (tool default).
const ACCEPT_SCORE: f64 = 0.88;
/// Lowest score offered as a "did you mean" candidate (tool default).
const SUGGEST_SCORE: f64 = 0.70;
/// Two different chemicals scoring within this margin of each other are a tie.
const TIE_MARGIN: f64 = 0.02;

/// Exposure scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    /// A spill, leak, or release (road crash, facility release, home heating oil tank).
    #[serde(alias = "spill", alias = "release", alias = "spill_release")]
    SpillOrRelease,
    /// Indoor air in a home, school, or public building.
    #[serde(alias = "indoor")]
    IndoorAir,
    /// A contaminated site or contaminated soil.
    #[serde(alias = "site", alias = "soil", alias = "contaminated_soil")]
    ContaminatedSite,
    /// Drinking water (public system or private well).
    #[serde(alias = "water", alias = "well")]
    DrinkingWater,
    /// A workplace exposure.
    #[serde(alias = "work", alias = "occupational")]
    Workplace,
}

impl Scenario {
    fn id(self) -> &'static str {
        match self {
            Scenario::SpillOrRelease => "spill_or_release",
            Scenario::IndoorAir => "indoor_air",
            Scenario::ContaminatedSite => "contaminated_site",
            Scenario::DrinkingWater => "drinking_water",
            Scenario::Workplace => "workplace",
        }
    }
}

/// Arguments for `chemical_exposure_reference`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct ChemicalArgs {
    /// The chemical: a name, synonym, or trade name ("benzene", "TCE", "perc", "PFOA") or a
    /// CAS Registry Number ("71-43-2").
    pub chemical: String,
    /// Exposure scenario for notes and whom to call: spill_or_release, indoor_air,
    /// contaminated_site, drinking_water, or workplace.
    #[serde(default)]
    pub scenario: Option<Scenario>,
}

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct ContactDef {
    id: String,
    name: String,
    phone: Option<String>,
    when: String,
    source: String,
    section: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct ScenarioDef {
    id: String,
    label: String,
    notes: Vec<Cited>,
    contacts: Vec<String>,
    links: Vec<LinkDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct AliasDef {
    alias: String,
    target: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ChemicalContent {
    sources: BTreeMap<String, DocSource>,
    occupational_limits: Vec<Cited>,
    mrls: Vec<Cited>,
    toxfaqs: Vec<Cited>,
    contacts: Vec<ContactDef>,
    scenarios: Vec<ScenarioDef>,
    aliases: Vec<AliasDef>,
}

// ---------------------------------------------------------------- output

/// How the query was matched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MatchInfo {
    /// Name of the matched entry.
    pub name: String,
    /// The name, synonym, or number that matched.
    pub matched_on: String,
    /// "cas", "exact", "alias", or "fuzzy".
    pub method: String,
    /// Similarity score (1 for exact and CAS matches).
    pub score: f64,
}

/// A NIOSH Pocket Guide entry as returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NioshEntry {
    pub name: String,
    pub url: String,
    pub synonyms: Vec<String>,
    pub cas: Vec<String>,
    /// DOT ID and Emergency Response Guidebook guide number.
    pub dot: String,
    pub conversion: String,
    /// NIOSH recommended exposure limit (workers; up to a 10-hour workday).
    pub rel: String,
    /// OSHA permissible exposure limit (workers; 8-hour shift).
    pub pel: String,
    /// IDLH as printed in the Pocket Guide.
    pub idlh_pocket_guide: String,
    /// Current value in the NIOSH IDLH table, when listed.
    pub idlh_current: Option<String>,
    pub idlh_documentation_url: Option<String>,
    pub physical_description: String,
    pub exposure_routes: String,
    pub symptoms: String,
    pub target_organs: String,
    pub cancer_site: String,
    pub first_aid: String,
    /// Date NIOSH last reviewed the page.
    pub last_reviewed: String,
}

/// One ATSDR minimal risk level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MrlOut {
    pub substance: String,
    /// Inhalation, oral, or external radiation.
    pub route: String,
    /// Acute (1-14 days), intermediate (15-364 days), or chronic (1 year or longer).
    pub duration: String,
    pub mrl: String,
    /// Total uncertainty and modifying factors.
    pub factors: String,
    /// Critical health endpoint (ATSDR's abbreviation).
    pub endpoint: String,
    /// "Final", or "Draft" (provisional).
    pub status: String,
    /// Cover date of the toxicological profile.
    pub cover_date: String,
}

/// A ToxFAQs fact sheet link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToxFaqLink {
    pub title: String,
    pub url: String,
    pub pdf_url: Option<String>,
}

/// An organization to call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContactOut {
    pub name: String,
    /// Main line (null for the local board of health: look up your town's).
    pub phone: Option<String>,
    pub when: String,
    pub citation: String,
}

/// Scenario notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScenarioOut {
    pub id: String,
    pub label: String,
    pub notes: Vec<CitedOut>,
    pub links: Vec<LinkDef>,
}

/// Result of `chemical_exposure_reference`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChemicalResult {
    pub query: String,
    pub matched: MatchInfo,
    /// CAS Registry Numbers of the matched chemical.
    pub cas: Vec<String>,
    /// The NIOSH Pocket Guide entry (occupational limits and first aid).
    pub niosh: Option<NioshEntry>,
    /// Other Pocket Guide entries with the same CAS number (names only).
    pub other_niosh_entries: Vec<String>,
    /// ATSDR minimal risk levels by route and duration.
    pub mrls: Vec<MrlOut>,
    /// ATSDR ToxFAQs fact sheets (web page and two-page PDF).
    pub toxfaqs: Vec<ToxFaqLink>,
    /// How to read occupational limits and MRLs (always included).
    pub how_to_read: Vec<CitedOut>,
    pub scenario: Option<ScenarioOut>,
    /// Whom to call (for the scenario, or general contacts).
    pub contacts: Vec<ContactOut>,
}

// ---------------------------------------------------------------- matching

/// A searchable chemical: a Pocket Guide entry, an MRL substance, or a ToxFAQ.
#[derive(Debug, Clone)]
struct Candidate {
    label: String,
    keys: Vec<String>,
    cas: Vec<String>,
    npg: Option<usize>,
    toxids: Vec<String>,
}

impl Candidate {
    /// What makes two candidates the same chemical: their CAS numbers, else
    /// their normalized label.
    fn identity(&self) -> String {
        if self.cas.is_empty() {
            phn_core::text::normalize(&self.label)
        } else {
            let mut c = self.cas.clone();
            c.sort();
            c.join(",")
        }
    }
}

/// Extra search keys for a name: without a parenthetical ("Nickel (as Ni)"),
/// the parenthetical abbreviation ("PFOA", "PCBs" and "PCB"), and the part after
/// a colon ("Mercury: Metallic Mercury").
fn name_keys(name: &str) -> Vec<String> {
    let mut keys = vec![name.to_string()];
    if let Some((before, rest)) = name.split_once('(')
        && let Some((inside, _)) = rest.split_once(')')
    {
        let b = before.trim();
        if !b.is_empty() {
            keys.push(b.to_string());
        }
        let inside = inside.trim();
        let is_abbrev = !inside.is_empty()
            && !inside.contains(' ')
            && inside.chars().filter(|c| c.is_ascii_uppercase()).count() >= 2;
        if is_abbrev {
            keys.push(inside.to_string());
            if let Some(singular) = inside.strip_suffix('s')
                && singular.chars().count() >= 3
            {
                keys.push(singular.to_string());
            }
        }
    }
    if let Some((_, after)) = name.split_once(": ") {
        keys.push(after.trim().to_string());
    }
    keys
}

fn candidates(r: &ChemicalReference, aliases: &[AliasDef]) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    for (i, e) in r.npg.iter().enumerate() {
        let mut keys = name_keys(&e.name);
        keys.extend(e.synonyms.iter().cloned());
        out.push(Candidate {
            label: e.name.clone(),
            keys,
            cas: e.cas.clone(),
            npg: Some(i),
            toxids: Vec::new(),
        });
    }
    // MRL substances (one candidate per substance name).
    let mut seen: Vec<String> = Vec::new();
    for m in &r.mrls {
        if seen.contains(&m.substance) {
            continue;
        }
        seen.push(m.substance.clone());
        out.push(Candidate {
            label: phn_core::text::title_case(&m.substance.to_lowercase()),
            keys: name_keys(&m.substance),
            cas: m.cas.clone(),
            npg: None,
            toxids: vec![m.toxid.clone()],
        });
    }
    for t in &r.toxfaqs {
        out.push(Candidate {
            label: t.title.clone(),
            keys: name_keys(&t.title),
            cas: t.cas.clone(),
            npg: None,
            toxids: vec![t.toxid.clone()],
        });
    }
    // Aliases from the content file attach to every candidate named like the target.
    for a in aliases {
        let target = phn_core::text::normalize(&a.target);
        for c in out.iter_mut() {
            if c.keys
                .iter()
                .any(|k| phn_core::text::normalize(k) == target)
            {
                c.keys.push(a.alias.clone());
            }
        }
    }
    out
}

/// The outcome of resolving a query.
enum Resolved {
    Found(Vec<usize>, MatchInfo),
    Ambiguous(Vec<String>),
    NotFound(Vec<String>),
}

fn looks_like_cas(q: &str) -> bool {
    let t = q.trim();
    !t.is_empty()
        && t.chars().all(|c| c.is_ascii_digit() || c == '-')
        && t.matches('-').count() == 2
}

fn resolve(query: &str, cands: &[Candidate], aliases: &[AliasDef]) -> Result<Resolved> {
    let q = query.trim();
    if looks_like_cas(q) {
        if chemicals::cas_numbers(q).len() != 1 || !chemicals::cas_check_digit_ok(q) {
            return Err(PhnError::InvalidInput(format!(
                "'{q}' is not a valid CAS Registry Number (the check digit does not match); \
                 check the number or give the chemical's name"
            )));
        }
        let hits: Vec<usize> = cands
            .iter()
            .enumerate()
            .filter(|(_, c)| c.cas.iter().any(|x| x == q))
            .map(|(i, _)| i)
            .collect();
        if hits.is_empty() {
            return Ok(Resolved::NotFound(Vec::new()));
        }
        let name = hits
            .iter()
            .find(|&&i| cands[i].npg.is_some())
            .map(|&i| cands[i].label.clone())
            .unwrap_or_else(|| cands[hits[0]].label.clone());
        return Ok(Resolved::Found(
            hits,
            MatchInfo {
                name,
                matched_on: q.to_string(),
                method: "cas".into(),
                score: 1.0,
            },
        ));
    }
    let nq = phn_core::text::normalize(q);
    if nq.is_empty() {
        return Err(PhnError::InvalidInput(
            "name a chemical or give its CAS number".into(),
        ));
    }
    let is_alias = aliases
        .iter()
        .any(|a| phn_core::text::normalize(&a.alias) == nq);
    let ranked = phn_core::text::rank(q, cands, |c| c.keys.clone(), SUGGEST_SCORE);
    let Some(top) = ranked.first() else {
        return Ok(Resolved::NotFound(Vec::new()));
    };
    // Every candidate within the tie margin of the top score.
    let near: Vec<&phn_core::text::Ranked> = ranked
        .iter()
        .filter(|r| r.score >= top.score - TIE_MARGIN)
        .collect();
    let mut identities: Vec<String> = near.iter().map(|r| cands[r.index].identity()).collect();
    identities.sort();
    identities.dedup();
    let distinct_labels = |list: &[&phn_core::text::Ranked]| -> Vec<String> {
        let mut seen_ids: Vec<String> = Vec::new();
        let mut labels = Vec::new();
        for r in list {
            let c = &cands[r.index];
            let id = c.identity();
            if seen_ids.contains(&id) {
                continue;
            }
            seen_ids.push(id);
            labels.push(match c.cas.first() {
                Some(cas) => format!("{} (CAS {cas})", c.label),
                None => c.label.clone(),
            });
        }
        labels
    };
    if identities.len() > 1 {
        let all: Vec<&phn_core::text::Ranked> = ranked.iter().take(12).collect();
        let mut labels = distinct_labels(&near);
        for l in distinct_labels(&all) {
            if labels.len() >= 6 {
                break;
            }
            if !labels.contains(&l) {
                labels.push(l);
            }
        }
        return Ok(Resolved::Ambiguous(labels));
    }
    if top.score < ACCEPT_SCORE {
        let all: Vec<&phn_core::text::Ranked> = ranked.iter().take(12).collect();
        let mut labels = distinct_labels(&all);
        labels.truncate(6);
        return Ok(Resolved::NotFound(labels));
    }
    let hits: Vec<usize> = near.iter().map(|r| r.index).collect();
    let primary = hits
        .iter()
        .find(|&&i| cands[i].npg.is_some())
        .copied()
        .unwrap_or(hits[0]);
    let exact = top.score >= 1.0 - 1e-9;
    Ok(Resolved::Found(
        hits,
        MatchInfo {
            name: cands[primary].label.clone(),
            matched_on: top.matched_on.clone(),
            method: if is_alias {
                "alias"
            } else if exact {
                "exact"
            } else {
                "fuzzy"
            }
            .into(),
            score: (top.score * 1000.0).round() / 1000.0,
        },
    ))
}

// ---------------------------------------------------------------- formatting

fn route_label(r: &str) -> String {
    match r.trim_end_matches('.') {
        "Inh" => "Inhalation".into(),
        "Oral" => "Oral".into(),
        "Rad" => "External radiation".into(),
        other => other.to_string(),
    }
}

fn duration_label(d: &str) -> String {
    match d.trim_end_matches('.') {
        "Acute" => "Acute (1-14 days)".into(),
        "Int" => "Intermediate (15-364 days)".into(),
        "Chr" | "Chronic" => "Chronic (1 year or longer)".into(),
        other => other.to_string(),
    }
}

fn mrl_out(m: &MrlRow) -> MrlOut {
    MrlOut {
        substance: m.substance.clone(),
        route: route_label(&m.route),
        duration: duration_label(&m.duration),
        mrl: m.mrl.clone(),
        factors: m.factors.clone(),
        endpoint: m.endpoint.clone(),
        status: m.status.clone(),
        cover_date: m.cover_date.clone(),
    }
}

fn niosh_out(e: &NpgEntry, r: &ChemicalReference) -> NioshEntry {
    let current = r
        .idlh
        .iter()
        .find(|row| row.cas.iter().any(|c| e.cas.contains(c)));
    NioshEntry {
        name: e.name.clone(),
        url: e.url.clone(),
        synonyms: e.synonyms.clone(),
        cas: e.cas.clone(),
        dot: e.dot.clone(),
        conversion: e.conversion.clone(),
        rel: e.rel.clone(),
        pel: e.pel.clone(),
        idlh_pocket_guide: e.idlh.clone(),
        idlh_current: current.map(|c| c.idlh.clone()),
        idlh_documentation_url: current
            .map(|c| c.documentation_url.clone())
            .filter(|u| !u.is_empty()),
        physical_description: e.physical_description.clone(),
        exposure_routes: e.exposure_routes.clone(),
        symptoms: e.symptoms.clone(),
        target_organs: e.target_organs.clone(),
        cancer_site: e.cancer_site.clone(),
        first_aid: e.first_aid.clone(),
        last_reviewed: e.last_reviewed.clone(),
    }
}

/// Digits and unit-free comparison of two IDLH strings ("2,000 ppm" vs "2000 ppm").
fn same_idlh(a: &str, b: &str) -> bool {
    let key = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '.')
            .collect::<String>()
            .to_ascii_lowercase()
    };
    let (ka, kb) = (key(a), key(b));
    ka.contains(&kb) || kb.contains(&ka)
}

fn contact_out(c: &ContactDef, sources: &BTreeMap<String, DocSource>) -> ContactOut {
    ContactOut {
        name: c.name.clone(),
        phone: c.phone.clone(),
        when: c.when.clone(),
        citation: format!(
            "{}, {}{}",
            sources
                .get(&c.source)
                .map(|s| s.title.as_str())
                .unwrap_or(c.source.as_str()),
            c.section,
            match c.confidence {
                Confidence::High => "",
                Confidence::Medium => " (medium confidence)",
                Confidence::Low => " (low confidence)",
            }
        ),
    }
}

/// The standing framing, shown first in every response.
pub const FRAMING: &str = "Occupational limits (NIOSH REL, OSHA PEL, IDLH) are set for adult \
     workers over a work shift; they are not safe levels for community or residential exposure. \
     ATSDR minimal risk levels (MRLs) are screening levels, not cleanup levels or diagnostic \
     thresholds.";

/// Look up a chemical.
pub fn chemical_exposure_reference(
    ctx: &Context,
    args: ChemicalArgs,
) -> Result<Envelope<ChemicalResult>> {
    let c: ChemicalContent = content::load(CHEMICAL_EXPOSURE)?;
    let r = ChemicalReference::shared(&ctx.snapshots)?;
    let query = args.chemical.trim().to_string();
    if query.is_empty() {
        return Err(PhnError::InvalidInput(
            "name a chemical (name, synonym, or CAS number)".into(),
        ));
    }
    if query.chars().count() > 200 {
        return Err(PhnError::InvalidInput(
            "give one chemical name or CAS number (200 characters at most)".into(),
        ));
    }
    let cands = candidates(r, &c.aliases);
    let (hits, matched) = match resolve(&query, &cands, &c.aliases)? {
        Resolved::Found(h, m) => (h, m),
        Resolved::Ambiguous(list) => {
            return Err(PhnError::InvalidInput(format!(
                "'{query}' matches more than one chemical: {}. Name one of them, or give its CAS \
                 number.",
                list.join("; ")
            )));
        }
        Resolved::NotFound(list) => {
            return Err(PhnError::NotFound(format!(
                "'{query}' was not found in the NIOSH Pocket Guide, the ATSDR MRL table, or the \
                 ATSDR ToxFAQs index{}. Check the spelling or give the CAS number; for a product, \
                 use the chemical names on its Safety Data Sheet (section 3).",
                if list.is_empty() {
                    String::new()
                } else {
                    format!(". Did you mean: {}?", list.join("; "))
                }
            )));
        }
    };

    // Gather the chemical's CAS numbers and ATSDR ids from every matched candidate.
    let mut cas: Vec<String> = Vec::new();
    let mut toxids: Vec<String> = Vec::new();
    let mut npg_idx: Vec<usize> = Vec::new();
    for &i in &hits {
        for x in &cands[i].cas {
            if !cas.contains(x) {
                cas.push(x.clone());
            }
        }
        for t in &cands[i].toxids {
            if !toxids.contains(t) {
                toxids.push(t.clone());
            }
        }
        if let Some(n) = cands[i].npg
            && !npg_idx.contains(&n)
        {
            npg_idx.push(n);
        }
    }
    // Pocket Guide entries sharing a CAS number.
    for (i, e) in r.npg.iter().enumerate() {
        if !npg_idx.contains(&i) && e.cas.iter().any(|x| cas.contains(x)) {
            npg_idx.push(i);
        }
    }
    let primary = npg_idx.first().map(|&i| &r.npg[i]);
    let niosh = primary.map(|e| niosh_out(e, r));
    let other_niosh_entries: Vec<String> = npg_idx
        .iter()
        .skip(1)
        .map(|&i| r.npg[i].name.clone())
        .collect();

    // MRLs: by CAS, or by ATSDR id when the chemical has no CAS number.
    let mrl_rows: Vec<&MrlRow> = r
        .mrls
        .iter()
        .filter(|m| {
            if cas.is_empty() {
                toxids.contains(&m.toxid)
            } else {
                m.cas.iter().any(|x| cas.contains(x))
            }
        })
        .collect();
    for m in &mrl_rows {
        if !toxids.contains(&m.toxid) {
            toxids.push(m.toxid.clone());
        }
    }
    let mrls: Vec<MrlOut> = mrl_rows.iter().map(|m| mrl_out(m)).collect();
    // ToxFAQs: by CAS or ATSDR id.
    let toxfaqs: Vec<ToxFaqLink> = r
        .toxfaqs
        .iter()
        .filter(|t| t.cas.iter().any(|x| cas.contains(x)) || toxids.contains(&t.toxid))
        .map(|t| ToxFaqLink {
            title: t.title.clone(),
            url: t.url.clone(),
            pdf_url: t.pdf_url.clone(),
        })
        .collect();

    // Framing, scenario, contacts.
    let mut how_to_read = content::cite_all(&c.occupational_limits, &c.sources);
    how_to_read.extend(content::cite_all(&c.mrls, &c.sources));
    how_to_read.extend(content::cite_all(&c.toxfaqs, &c.sources));
    let scenario_def = args
        .scenario
        .and_then(|s| c.scenarios.iter().find(|d| d.id == s.id()));
    let contact_ids: Vec<String> = match scenario_def {
        Some(d) => d.contacts.clone(),
        None => [
            "emergency_911",
            "poison_control",
            "massdep_spill",
            "dph_toxicology",
            "local_board_of_health",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    };
    let contacts: Vec<ContactOut> = contact_ids
        .iter()
        .filter_map(|id| c.contacts.iter().find(|x| &x.id == id))
        .map(|x| contact_out(x, &c.sources))
        .collect();
    let scenario = scenario_def.map(|d| ScenarioOut {
        id: d.id.clone(),
        label: d.label.clone(),
        notes: content::cite_all(&d.notes, &c.sources),
        links: d.links.clone(),
    });

    // Summary.
    let mut s = format!(
        "**{}**{}\n\n",
        matched.name,
        if cas.is_empty() {
            String::new()
        } else {
            format!(" (CAS {})", cas.join(", "))
        }
    );
    if matched.method == "fuzzy" {
        s.push_str(&format!(
            "Closest match for \"{query}\" (matched \"{}\", score {:.2}); check this is the \
             chemical you meant.\n\n",
            matched.matched_on, matched.score
        ));
    }
    s.push_str(&format!("> **Read this first.** {FRAMING}\n\n"));
    if let Some(n) = &niosh {
        s.push_str(&format!(
            "**NIOSH Pocket Guide: {}** ({}; page last reviewed {})\n\n",
            n.name, n.url, n.last_reviewed
        ));
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut row = |k: &str, v: &str| {
            if !v.trim().is_empty() {
                rows.push(vec![k.to_string(), v.to_string()]);
            }
        };
        let syn: Vec<String> = n.synonyms.iter().take(8).cloned().collect();
        row(
            "Synonyms",
            &format!(
                "{}{}",
                syn.join(", "),
                if n.synonyms.len() > 8 { ", ..." } else { "" }
            ),
        );
        row("NIOSH REL (workers)", &n.rel);
        row("OSHA PEL (workers)", &n.pel);
        row("IDLH (Pocket Guide)", &n.idlh_pocket_guide);
        if let Some(cur) = &n.idlh_current {
            row("IDLH (current NIOSH table)", cur);
        }
        row("DOT ID and ERG guide", &n.dot);
        row("Conversion", &n.conversion);
        row("Physical description", &n.physical_description);
        row("Exposure routes", &n.exposure_routes);
        row("Symptoms", &n.symptoms);
        row("Target organs", &n.target_organs);
        row("Cancer site", &n.cancer_site);
        row("First aid", &n.first_aid);
        s.push_str(&md::table(&["", "NIOSH"], &rows));
        if !other_niosh_entries.is_empty() {
            s.push_str(&format!(
                "\nOther Pocket Guide entries with this CAS number: {}.\n",
                other_niosh_entries.join("; ")
            ));
        }
        s.push('\n');
    } else {
        s.push_str(
            "**NIOSH Pocket Guide:** no entry for this chemical (the Pocket Guide covers 677 \
             workplace chemicals).\n\n",
        );
    }
    let mrl_vintage = r
        .mrl_source
        .effective_date
        .clone()
        .unwrap_or_else(|| "current".into());
    if mrls.is_empty() {
        s.push_str(&format!(
            "**ATSDR minimal risk levels:** none listed in the ATSDR MRL table ({mrl_vintage}).\n\n"
        ));
    } else {
        s.push_str(&format!(
            "**ATSDR minimal risk levels** (screening levels; MRL table {mrl_vintage})\n\n"
        ));
        let rows: Vec<Vec<String>> = mrls
            .iter()
            .map(|m| {
                vec![
                    m.substance.clone(),
                    m.route.clone(),
                    m.duration.clone(),
                    m.mrl.clone(),
                    m.endpoint.clone(),
                    m.status.clone(),
                    m.factors.clone(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "Substance",
                "Route",
                "Duration",
                "MRL",
                "Endpoint",
                "Status",
                "Total factors",
            ],
            &rows,
        ));
        s.push('\n');
    }
    if toxfaqs.is_empty() {
        s.push_str("**ATSDR ToxFAQs:** no fact sheet for this chemical.\n\n");
    } else {
        s.push_str("**ATSDR ToxFAQs** (plain-language fact sheets to share):\n");
        for t in &toxfaqs {
            s.push_str(&format!(
                "- {}: {}{}\n",
                t.title,
                t.url,
                t.pdf_url
                    .as_ref()
                    .map(|p| format!(" (PDF: {p})"))
                    .unwrap_or_default()
            ));
        }
        s.push('\n');
    }
    if let Some(sc) = &scenario {
        s.push_str(&format!("**Scenario: {}**\n", sc.label));
        for n in &sc.notes {
            s.push_str(&content::bullet(n));
        }
        for l in &sc.links {
            s.push_str(&format!("- {}: {}\n", l.label, l.url));
        }
        s.push('\n');
    }
    s.push_str("**Whom to call**\n\n");
    s.push_str(&md::table(
        &["Who", "Phone", "When"],
        &contacts
            .iter()
            .map(|x| {
                vec![
                    x.name.clone(),
                    x.phone
                        .clone()
                        .unwrap_or_else(|| "your town's number".into()),
                    x.when.clone(),
                ]
            })
            .collect::<Vec<_>>(),
    ));
    s.push_str("\n**How to read these numbers**\n");
    for h in &how_to_read {
        s.push_str(&content::bullet(h));
    }

    // Caveats.
    let mut caveats = vec![FRAMING.to_string()];
    if mrls.iter().any(|m| m.status.eq_ignore_ascii_case("draft")) {
        caveats.push(
            "Some MRLs come from a draft toxicological profile and are provisional; they may \
             change when the profile is final."
                .into(),
        );
    }
    if mrls.is_empty() {
        caveats.push(
            "No ATSDR MRL is listed for this chemical. ATSDR derives MRLs only when it judges the \
             data sufficient, so a missing MRL does not mean the chemical is safe."
                .into(),
        );
    }
    if let Some(n) = &niosh
        && let Some(cur) = &n.idlh_current
        && !same_idlh(cur, &n.idlh_pocket_guide)
    {
        caveats.push(format!(
            "The Pocket Guide prints IDLH \"{}\"; NIOSH's current IDLH table lists \"{cur}\". \
             NIOSH treats a value based on its Current Intelligence Bulletin 66 as current \
             policy.",
            n.idlh_pocket_guide
        ));
    }
    if matched.method == "fuzzy" {
        caveats.push(format!(
            "\"{query}\" was matched to {} by spelling similarity; confirm the chemical (the CAS \
             number on the Safety Data Sheet is the surest way).",
            matched.name
        ));
    }
    if args.scenario.is_none() {
        caveats.push(
            "No exposure scenario was given; add one (spill_or_release, indoor_air, \
             contaminated_site, drinking_water, workplace) for scenario notes and contacts."
                .into(),
        );
    }
    caveats.push(
        "Reference data are bundled snapshots (NIOSH Pocket Guide pages, the NIOSH IDLH table, \
         the ATSDR MRL table, and the ToxFAQs index); check the linked pages for updates before \
         advising residents."
            .into(),
    );
    caveats.extend(content::review_caveats(&[CHEMICAL_EXPOSURE]));

    // Sources.
    let mut sources: Vec<Source> = Vec::new();
    if let Some(e) = primary {
        let mut src = r.npg_source.clone();
        src.title = format!("NIOSH Pocket Guide to Chemical Hazards: {}", e.name);
        src.url = Some(e.url.clone());
        src.effective_date = Some(e.last_reviewed.clone()).filter(|d| !d.is_empty());
        sources.push(src);
        if niosh.as_ref().is_some_and(|n| n.idlh_current.is_some()) {
            sources.push(r.idlh_source.clone());
        }
    }
    if !mrls.is_empty() {
        sources.push(r.mrl_source.clone());
    }
    if !toxfaqs.is_empty() {
        sources.push(r.toxfaqs_source.clone());
    }
    let mut keys: Vec<&str> = c
        .occupational_limits
        .iter()
        .chain(&c.mrls)
        .chain(&c.toxfaqs)
        .map(|x| x.source.as_str())
        .collect();
    if let Some(d) = scenario_def {
        keys.extend(d.notes.iter().map(|n| n.source.as_str()));
    }
    for id in &contact_ids {
        if let Some(x) = c.contacts.iter().find(|x| &x.id == id) {
            keys.push(x.source.as_str());
        }
    }
    sources.extend(content::sources_for(keys, &c.sources));
    debug_assert!(sources.iter().all(|s| s.mode == SourceMode::Snapshot));

    Ok(Envelope::new(
        s,
        ChemicalResult {
            query,
            matched,
            cas,
            niosh,
            other_niosh_entries,
            mrls,
            toxfaqs,
            how_to_read,
            scenario,
            contacts,
        },
    )
    .sources(sources)
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

    fn run(c: &Context, chemical: &str, scenario: Option<Scenario>) -> Result<Envelope<ChemicalResult>> {
        chemical_exposure_reference(
            c,
            ChemicalArgs {
                chemical: chemical.into(),
                scenario,
            },
        )
    }

    #[test]
    fn name_keys_split_parentheticals_and_colons() {
        let k = name_keys("Polychlorinated Biphenyls (PCBs)");
        assert!(k.contains(&"Polychlorinated Biphenyls".to_string()));
        assert!(k.contains(&"PCBs".to_string()));
        assert!(k.contains(&"PCB".to_string()));
        let k = name_keys("Nickel metal and other compounds (as Ni)");
        assert!(k.contains(&"Nickel metal and other compounds".to_string()));
        assert!(!k.contains(&"as Ni".to_string()));
        assert!(name_keys("Mercury: Metallic Mercury").contains(&"Metallic Mercury".to_string()));
    }

    #[test]
    fn labels_expand_mrl_codes() {
        assert_eq!(route_label("Inh."), "Inhalation");
        assert_eq!(duration_label("Int."), "Intermediate (15-364 days)");
        assert_eq!(duration_label("Chr."), "Chronic (1 year or longer)");
        assert!(same_idlh("2,000 ppm", "Ca [2000 ppm]"));
        assert!(!same_idlh("137 ppm", "500 ppm"));
    }

    #[test]
    fn benzene_by_name_and_cas_has_niosh_mrls_and_toxfaq() {
        let (_d, c) = ctx();
        let env = run(&c, "Benzene", Some(Scenario::SpillOrRelease)).unwrap();
        let d = &env.data;
        assert_eq!(d.matched.method, "exact");
        assert_eq!(d.cas, vec!["71-43-2"]);
        let n = d.niosh.as_ref().unwrap();
        assert!(n.rel.contains("Ca"));
        assert!(!d.mrls.is_empty());
        assert!(d.mrls.iter().any(|m| m.route == "Inhalation"));
        assert!(d.toxfaqs.iter().any(|t| t.title == "Benzene"));
        assert!(env.summary.contains("not safe levels for community"));
        assert!(env.summary.contains("1-888-304-1133"));
        assert!(env.summary.contains("1-800-222-1222"));
        assert!(env.caveats[0].contains("screening levels"));
        assert!(env.notice.is_some());
        let by_cas = run(&c, "71-43-2", None).unwrap();
        assert_eq!(by_cas.data.matched.method, "cas");
        assert_eq!(by_cas.data.niosh.as_ref().unwrap().name, n.name);
        assert!(by_cas.caveats.iter().any(|x| x.contains("No exposure scenario")));
    }

    #[test]
    fn synonyms_aliases_and_typos_resolve() {
        let (_d, c) = ctx();
        let tce = run(&c, "TCE", None).unwrap();
        assert!(tce.data.cas.contains(&"79-01-6".to_string()), "{:?}", tce.data.matched);
        let pfas = run(&c, "PFAS", Some(Scenario::DrinkingWater)).unwrap();
        assert_eq!(pfas.data.matched.method, "alias");
        assert!(pfas.data.toxfaqs.iter().any(|t| t.title == "Perfluoroalkyls"));
        assert!(pfas.summary.contains("drinking_water_quality_lookup"));
        let pfoa = run(&c, "PFOA", None).unwrap();
        assert!(pfoa.data.mrls.iter().any(|m| m.route == "Oral"));
        let typo = run(&c, "tolueen", None).unwrap();
        assert_eq!(typo.data.matched.method, "fuzzy");
        assert!(typo.data.cas.contains(&"108-88-3".to_string()));
    }

    #[test]
    fn bad_and_ambiguous_queries_are_errors() {
        let (_d, c) = ctx();
        let e = run(&c, "71-43-3", None).unwrap_err().to_string();
        assert!(e.contains("check digit"), "{e}");
        assert!(run(&c, "", None).is_err());
        assert!(run(&c, "zzqx flurble", None).is_err());
        // "xylene" isomers: the ToxFAQ "Xylene" is an exact match, so no tie there;
        // a bare "dichlorobenzene" names several isomers.
        let e = run(&c, "dichlorobenzene", None);
        if let Err(err) = e {
            assert!(err.to_string().contains("more than one") || err.to_string().contains("Did you mean"));
        }
    }

    #[test]
    fn every_scenario_has_contacts_with_numbers() {
        let (_d, c) = ctx();
        for s in [
            Scenario::SpillOrRelease,
            Scenario::IndoorAir,
            Scenario::ContaminatedSite,
            Scenario::DrinkingWater,
            Scenario::Workplace,
        ] {
            let env = run(&c, "chlorine", Some(s)).unwrap();
            let sc = env.data.scenario.as_ref().unwrap();
            assert_eq!(sc.id, s.id());
            assert!(!sc.notes.is_empty());
            assert!(!env.data.contacts.is_empty());
            assert!(env.data.contacts.iter().any(|x| x.phone.is_some()));
        }
    }
}
