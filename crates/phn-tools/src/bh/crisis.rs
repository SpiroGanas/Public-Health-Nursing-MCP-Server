//! MH-01 `find_crisis_resources`: statewide crisis lines with phone, text,
//! chat, and language options, plus the Community Behavioral Health Center(s)
//! for a town.
//!
//! Lines come from `content/bh/crisis_resources.yaml` (pending review); the
//! CBHC for each town comes from the `ma-cbhc-directory` snapshot (built from
//! mass.gov by `cargo xtask build-service-directories`).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, Privacy, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::aging::content::{
    self as c10, CRISIS, CitedOut, CitedText, Confidence, SourceRef, cite_all,
};
use crate::aging::directory::{CbhcMatch, Directory};
use crate::context::Context;
use crate::md;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Contact {
    /// call, text, chat, or web.
    pub mode: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LanguageOption {
    pub language: String,
    pub how: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LineDef {
    id: String,
    name: String,
    #[allow(dead_code)]
    audience: String,
    purpose: String,
    contacts: Vec<Contact>,
    languages: Vec<LanguageOption>,
    hours: String,
    cost: String,
    notes: Vec<String>,
    source: String,
    section: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct Warmlines {
    status: String,
    text: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CrisisContent {
    #[allow(dead_code)]
    meta: ContentMeta,
    sources: BTreeMap<String, SourceRef>,
    lines: Vec<LineDef>,
    warmlines: Warmlines,
    cbhc_services: Vec<CitedText>,
}

static CONTENT: LazyLock<CrisisContent> =
    LazyLock::new(|| c10::load(CRISIS).expect("crisis resources content parses"));

/// Who the resources are for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    /// Children and adolescents (under 18).
    Youth,
    Adult,
    #[default]
    All,
}

/// Arguments for `find_crisis_resources`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CrisisArgs {
    /// Massachusetts city or town (default: the configured town). Without a
    /// town, only the statewide lines are listed.
    pub town: Option<String>,
    /// youth, adult, or all (default).
    pub audience: Option<Audience>,
    /// A language the person prefers (e.g. "Spanish", "Portuguese",
    /// "Haitian Creole", "ASL"); the answer says how to get it on each line.
    pub language: Option<String>,
}

/// A crisis line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LineOut {
    pub id: String,
    pub name: String,
    pub purpose: String,
    pub contacts: Vec<Contact>,
    pub languages: Vec<LanguageOption>,
    pub hours: String,
    pub cost: String,
    pub notes: Vec<String>,
    pub citation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

/// A local Community Behavioral Health Center.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CbhcOut {
    pub name: String,
    pub address: String,
    pub phone: String,
    /// The "Serving" list as published (empty when none is published).
    pub serving_as_published: String,
}

/// Result of `find_crisis_resources`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CrisisResult {
    pub town: Option<String>,
    pub county: Option<String>,
    pub audience: Audience,
    pub lines: Vec<LineOut>,
    /// The town's CBHC(s) for walk-in crisis care, mobile crisis, and
    /// community crisis stabilization.
    pub local_cbhcs: Vec<CbhcOut>,
    /// How the CBHCs were matched to the town.
    pub cbhc_match: Option<CbhcMatch>,
    /// How to reach each line in the requested language.
    pub language_options: Vec<String>,
    pub youth_notes: Vec<String>,
    pub cbhc_services: Vec<CitedOut>,
    /// Warmline status (none are listed unless verified).
    pub warmlines: String,
    /// Which line to use when.
    pub when_to_use: Vec<String>,
}

fn language_plan(lines: &[LineOut], lang: &str) -> Vec<String> {
    let l = phn_core::text::normalize(lang);
    let is = |keys: &[&str]| keys.iter().any(|k| l.contains(k));
    let mut out = Vec::new();
    for line in lines {
        let pick = line.languages.iter().find(|o| {
            let n = phn_core::text::normalize(&o.language);
            (is(&["spanish", "espanol"]) && n.contains("spanish"))
                || (is(&["asl", "sign", "deaf"]) && n.contains("sign language"))
                || (is(&["tty", "relay", "hard of hearing"]) && n.contains("tty"))
                || n.contains(&l)
        });
        let generic = line
            .languages
            .iter()
            .find(|o| o.language.contains("languages"));
        match (pick, generic) {
            (Some(o), _) => out.push(format!("{}: {} ({})", line.name, o.how, o.language)),
            (None, Some(g)) => out.push(format!("{}: {} ({})", line.name, g.how, g.language)),
            (None, None) if line.id == "mobile_crisis" => out.push(format!(
                "{}: no language options are published on its mass.gov page; the Behavioral Health Help Line (200+ languages) can connect the person to the local CBHC.",
                line.name
            )),
            _ => {}
        }
    }
    out
}

/// List crisis resources for a town.
pub fn find_crisis_resources(ctx: &Context, args: CrisisArgs) -> Result<Envelope<CrisisResult>> {
    let c = &*CONTENT;
    let audience = args.audience.unwrap_or_default();
    let town_name = c10::town_or_configured(ctx, args.town.as_deref());
    let town = match &town_name {
        Some(t) => Some(c10::resolve_town(ctx, t)?),
        None => None,
    };
    let lines: Vec<LineOut> = c
        .lines
        .iter()
        .map(|l| LineOut {
            id: l.id.clone(),
            name: l.name.clone(),
            purpose: l.purpose.trim().to_string(),
            contacts: l.contacts.clone(),
            languages: l.languages.clone(),
            hours: l.hours.clone(),
            cost: l.cost.clone(),
            notes: l.notes.clone(),
            citation: c10::citation(&c.sources, &l.source, &l.section),
            confidence: Some(l.confidence).filter(|x| *x != Confidence::High),
        })
        .collect();

    let mut caveats = Vec::new();
    let mut sources = vec![c10::content_source(CRISIS)?];
    sources.extend(c10::sources_for(
        &c.sources,
        c.lines
            .iter()
            .map(|l| l.source.as_str())
            .chain(c.cbhc_services.iter().map(|s| s.source.as_str())),
    ));
    let mut local = Vec::new();
    let mut how = None;
    if let Some(m) = town {
        let dir = Directory::load(&ctx.snapshots)?;
        let (found, kind) = dir.cbhcs_for(&m.name, &m.county);
        how = Some(kind);
        local = found
            .iter()
            .map(|x| CbhcOut {
                name: x.name.clone(),
                address: x.address.clone(),
                phone: x.phone.clone(),
                serving_as_published: x.served_as_published.clone(),
            })
            .collect();
        sources.push(dir.cbhc_source.clone());
        if kind == CbhcMatch::SameCounty {
            caveats.push(format!(
                "{} is not named in any CBHC's published 'Serving' list, so the CBHCs in {} County are shown. The mobile crisis line (877-382-1609, enter the ZIP code) and the Behavioral Health Help Line route callers to the nearest CBHC.",
                m.name, m.county
            ));
        }
        caveats.extend(dir.cbhc_caveats.iter().skip(1).cloned());
    }
    let language_options = args
        .language
        .as_deref()
        .filter(|l| !l.trim().is_empty())
        .map(|l| language_plan(&lines, l))
        .unwrap_or_default();
    let youth_notes = if audience != Audience::Adult {
        vec![
            "Mobile Crisis Intervention serves children and youth as well as adults: a CBHC team can come to the home or school within 60 minutes when appropriate (877-382-1609).".to_string(),
            "CBHC community crisis stabilization has separate youth and adult programs.".to_string(),
            "For a student in crisis at school, follow the school's crisis protocol and involve the parent or guardian as it directs.".to_string(),
        ]
    } else {
        Vec::new()
    };
    let when_to_use = vec![
        "Immediate danger (attempt in progress, weapon, medical emergency): call 911.".to_string(),
        "Someone in emotional distress or thinking about suicide, or worried about someone: call or text 988, or chat.".to_string(),
        "A mental health or substance use crisis needing in-person help within an hour, for a child or adult: Mobile Crisis Intervention (877-382-1609, enter the ZIP code) or walk in to the local CBHC.".to_string(),
        "Finding treatment (therapy, medication, substance use care) or not sure where to start: the Behavioral Health Help Line (833-773-2445, call or text, 24/7, 200+ languages).".to_string(),
    ];
    let cbhc_services = cite_all(&c.sources, &c.cbhc_services);

    // ---- markdown
    let mut s = format!(
        "**Crisis resources{}**\n\n",
        town.map(|m| format!(" — {} ({} County)", m.name, m.county))
            .unwrap_or_else(|| " — Massachusetts (statewide)".into())
    );
    s.push_str("**When to use which**\n");
    for w in &when_to_use {
        s.push_str(&format!("- {w}\n"));
    }
    s.push('\n');
    let rows: Vec<Vec<String>> = lines
        .iter()
        .map(|l| {
            vec![
                l.name.clone(),
                l.contacts
                    .iter()
                    .map(|c| format!("{}: {}", c.mode, c.value))
                    .collect::<Vec<_>>()
                    .join("; "),
                l.hours.clone(),
                if l.languages.is_empty() {
                    "—".into()
                } else {
                    l.languages
                        .iter()
                        .map(|o| o.language.clone())
                        .collect::<Vec<_>>()
                        .join("; ")
                },
            ]
        })
        .collect();
    s.push_str(&md::table(&["Line", "How to reach", "Hours", "Languages and access"], &rows));
    if let Some(m) = town {
        s.push_str(&format!(
            "\n**Community Behavioral Health Center(s) for {}**{}\n",
            m.name,
            match how {
                Some(CbhcMatch::SameCounty) => format!(" (town not in a published list; CBHCs in {} County)", m.county),
                _ => String::new(),
            }
        ));
        for x in &local {
            s.push_str(&format!("- **{}**, {} — {}\n", x.name, x.address, x.phone));
        }
        s.push_str("\nCBHCs offer 24/7 walk-in crisis care, mobile crisis, and short-term crisis stabilization to anyone in Massachusetts, insured or not.\n");
    } else {
        s.push_str("\nGive a town (or set one with configure_jurisdiction) to see the local Community Behavioral Health Center.\n");
    }
    if !language_options.is_empty() {
        s.push_str("\n**Language access**\n");
        for o in &language_options {
            s.push_str(&format!("- {o}\n"));
        }
    }
    if !youth_notes.is_empty() {
        s.push_str("\n**Youth**\n");
        for y in &youth_notes {
            s.push_str(&format!("- {y}\n"));
        }
    }
    s.push_str(&format!("\n_Warmlines: {}_\n", c.warmlines.text.trim()));

    caveats.push("Phone numbers were read from mass.gov and NIMH on the dates shown; call-test them before printing them for the public.".into());
    if c.warmlines.status != "verified" {
        caveats.push(format!(
            "No peer warmline is listed: none could be verified ({}).",
            c10::citation(&c.sources, &c.warmlines.source, &c.warmlines.section)
        ));
    }
    caveats.extend(c10::review_caveats(&[CRISIS]));
    Ok(Envelope::new(
        s,
        CrisisResult {
            town: town.map(|m| m.name.clone()),
            county: town.map(|m| m.county.clone()),
            audience,
            lines,
            local_cbhcs: local,
            cbhc_match: how,
            language_options,
            youth_notes,
            cbhc_services,
            warmlines: c.warmlines.text.trim().to_string(),
            when_to_use,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy::none()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_sources_are_known() {
        let c = &*CONTENT;
        for l in &c.lines {
            c10::assert_keys_known(&c.sources, &[&l.source], &l.id);
        }
        for s in &c.cbhc_services {
            c10::assert_keys_known(&c.sources, &[&s.source], "cbhc_services");
        }
        let l988 = c.lines.iter().find(|l| l.id == "lifeline_988").unwrap();
        assert!(l988.contacts.iter().any(|c| c.mode == "text" && c.value == "988"));
        let bhhl = c.lines.iter().find(|l| l.id == "bhhl").unwrap();
        assert_eq!(bhhl.contacts[0].value, "(833) 773-2445");
    }
}
