//! MH-04 `find_naloxone_and_harm_reduction`: how to get naloxone in
//! Massachusetts (pharmacy standing order, distribution programs, agencies),
//! the standing order's formulations, syringe service program approval for a
//! town, fentanyl test strip facts, contacts, and a naloxone teaching script,
//! with an optional Word handout written to /data/output.
//!
//! Content: `content/bh/substance_use_resources.yaml` (DPH standing order,
//! pharmacy policy, naloxone pages, OEND core competencies, SSP locator list,
//! CDC pages). DPH's site maps (Tableau and Google My Maps) cannot be read, so
//! the tool links to them rather than listing sites.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::text::normalize;
use phn_core::{Envelope, PhnError, Privacy, Result};
use phn_docs::model::{Block, Document, Inline};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::comms::content::{self, NamedSource, SUBSTANCE_USE};
use crate::context::Context;

// ---------------------------------------------------------------- content

/// A cited statement in the substance use content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CitedText {
    pub text: String,
    /// Source key in the content file.
    pub source: String,
    pub section: String,
    /// Developer confidence when below high.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
}

/// A phone or web contact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Contact {
    pub name: String,
    pub detail: String,
    pub source: String,
}

/// A naloxone formulation in the statewide standing order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Formulation {
    pub name: String,
    pub dispense: String,
    pub directions: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct StandingOrder {
    pub signed: String,
    pub authority: String,
    pub formulations: Vec<Formulation>,
    pub why_preferred: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SyringeServices {
    pub statute: String,
    pub statute_source: String,
    pub list_source: String,
    pub list_note: String,
    pub approved_towns: Vec<String>,
}

/// One step of the teaching script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptStep {
    pub step: String,
    pub text: String,
    /// Source keys.
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ResourceContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, NamedSource>,
    pub contacts: Vec<Contact>,
    pub standing_order: StandingOrder,
    pub access: Vec<CitedText>,
    pub pharmacy_rules: Vec<CitedText>,
    pub legal: Vec<CitedText>,
    pub syringe_services: SyringeServices,
    pub fentanyl_test_strips: Vec<CitedText>,
    pub risk_factors: Vec<CitedText>,
    pub treatment: Vec<CitedText>,
    pub teaching_script: Vec<ScriptStep>,
}

pub(crate) static RESOURCES: LazyLock<ResourceContent> = LazyLock::new(|| {
    content::load(SUBSTANCE_USE)
        .expect("content/bh/substance_use_resources.yaml matches its schema")
});

fn source_title(key: &str) -> String {
    RESOURCES
        .sources
        .get(key)
        .map(|s| s.title.clone())
        .unwrap_or_else(|| key.to_string())
}

fn url_of(key: &str) -> String {
    RESOURCES
        .sources
        .get(key)
        .and_then(|s| s.url.clone())
        .unwrap_or_default()
}

// ---------------------------------------------------------------- arguments and results

/// Arguments for `find_naloxone_and_harm_reduction`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct HarmReductionArgs {
    /// A Massachusetts city or town (default: your configured town). Used for syringe service
    /// program approval and nearby approved towns in the same county.
    pub town: Option<String>,
    /// Include the naloxone teaching script (default true).
    pub include_teaching_script: Option<bool>,
    /// Write the teaching script and resources as an accessible Word handout to /data/output
    /// ("docx").
    pub handout: Option<String>,
}

/// Syringe service program approval for a town.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SyringeServiceStatus {
    pub town: String,
    pub county: String,
    /// The town's board of health has approved a syringe services program (DPH list).
    pub approved: bool,
    /// Other towns in the same county with approval.
    pub approved_in_county: Vec<String>,
    pub note: String,
    pub statute: String,
    pub locator_url: String,
}

/// Result of `find_naloxone_and_harm_reduction`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HarmReductionResult {
    pub town: Option<String>,
    pub ways_to_get_naloxone: Vec<CitedText>,
    pub standing_order_signed: String,
    pub standing_order_authority: String,
    pub standing_order_formulations: Vec<Formulation>,
    pub pharmacy_rules: Vec<CitedText>,
    pub legal_protections: Vec<CitedText>,
    pub syringe_services: Option<SyringeServiceStatus>,
    pub fentanyl_test_strips: Vec<CitedText>,
    pub overdose_risk_factors: Vec<CitedText>,
    pub teaching_script: Vec<ScriptStep>,
    pub contacts: Vec<Contact>,
    /// Maps the server cannot read; open them for sites near the town.
    pub locators: Vec<String>,
    pub handout_file: Option<String>,
}

// ---------------------------------------------------------------- the tool

/// Naloxone access, harm reduction resources, and a teaching script.
pub fn find_naloxone_and_harm_reduction(
    ctx: &Context,
    args: HarmReductionArgs,
) -> Result<Envelope<HarmReductionResult>> {
    let r = &*RESOURCES;
    let town_q = args
        .town
        .clone()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| ctx.settings().jurisdiction.town);
    let syringe = match town_q.as_deref() {
        None => None,
        Some(q) => {
            let m = ctx.ma.town(q).ok_or_else(|| {
                let s: Vec<String> = ctx
                    .ma
                    .suggest_towns(q, 3)
                    .into_iter()
                    .map(|t| t.name.clone())
                    .collect();
                PhnError::NotFound(format!(
                    "'{q}' is not a Massachusetts city or town{}",
                    if s.is_empty() {
                        String::new()
                    } else {
                        format!(". Did you mean: {}?", s.join(", "))
                    }
                ))
            })?;
            let key = |n: &str| normalize(&n.replace('-', " "));
            let approved_set: Vec<String> = r
                .syringe_services
                .approved_towns
                .iter()
                .map(|t| key(t))
                .collect();
            let approved = approved_set.contains(&key(&m.name));
            let in_county: Vec<String> = r
                .syringe_services
                .approved_towns
                .iter()
                .filter(|t| key(t) != key(&m.name))
                .filter(|t| ctx.ma.town(t).is_some_and(|o| o.county == m.county))
                .cloned()
                .collect();
            Some(SyringeServiceStatus {
                town: m.name.clone(),
                county: m.county.clone(),
                approved,
                approved_in_county: in_county,
                note: r.syringe_services.list_note.clone(),
                statute: r.syringe_services.statute.clone(),
                locator_url: url_of(&r.syringe_services.list_source),
            })
        }
    };
    let include_script = args.include_teaching_script.unwrap_or(true);
    let locators = vec![
        format!(
            "{}: {}",
            source_title("naloxone_locator"),
            url_of("naloxone_locator")
        ),
        format!("{}: {}", source_title("ssp_locator"), url_of("ssp_locator")),
        format!("{}: {}", source_title("youcan"), url_of("youcan")),
    ];

    // ---- summary
    let mut summary = String::from(
        "**Naloxone and harm reduction in Massachusetts**\n\n**Ways to get naloxone**\n",
    );
    for a in &r.access {
        summary.push_str(&format!("- {}\n", a.text));
    }
    summary.push_str(&format!(
        "\n**Statewide standing order** (signed {}): pharmacists may dispense naloxone rescue kits without a prescription. Preferred formulations: {}. {}\n",
        r.standing_order.signed,
        r.standing_order
            .formulations
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>()
            .join("; "),
        r.standing_order.why_preferred
    ));
    if let Some(s) = &syringe {
        summary.push_str(&format!(
            "\n**Syringe services, {} ({} County):** {}. Other {} County towns with approval: {}. {} Map: {}\n",
            s.town,
            s.county,
            if s.approved {
                "the board of health has approved a syringe services program (DPH list)"
            } else {
                "not on DPH's list of towns whose board of health has approved a syringe services program"
            },
            s.county,
            if s.approved_in_county.is_empty() { "none".to_string() } else { s.approved_in_county.join(", ") },
            s.note,
            s.locator_url
        ));
    }
    summary.push_str("\n**Fentanyl test strips**\n");
    for f in &r.fentanyl_test_strips {
        summary.push_str(&format!(
            "- {}{}\n",
            f.text,
            match f.confidence.as_deref() {
                Some("low") => " _(not verified)_",
                _ => "",
            }
        ));
    }
    if include_script {
        summary.push_str("\n**Naloxone teaching script** (from DPH's OEND core competencies, the 2026 standing order, and CDC)\n");
        for (i, s) in r.teaching_script.iter().enumerate() {
            summary.push_str(&format!("{}. **{}.** {}\n", i + 1, s.step, s.text));
        }
    }
    summary.push_str("\n**Contacts**\n");
    for c in &r.contacts {
        summary.push_str(&format!("- {}: {}\n", c.name, c.detail));
    }
    summary.push_str(&format!(
        "\nFind sites near you (the server cannot read these maps): {}\n",
        locators.join("; ")
    ));

    // ---- optional handout
    let mut handout_file = None;
    let mut privacy = Privacy::none();
    if let Some(fmt) = args
        .handout
        .as_deref()
        .map(|f| f.trim().to_ascii_lowercase())
        .filter(|f| !f.is_empty() && f != "none")
    {
        if fmt != "docx" {
            return Err(PhnError::InvalidInput("handout must be \"docx\"".into()));
        }
        let mut blocks = vec![
            Block::Paragraph(vec![Inline::plain(
                "An overdose is a medical emergency. Call 911, give naloxone, and stay with the person.",
            )]),
            Block::Heading {
                level: 2,
                text: "How to respond to an overdose".into(),
            },
            Block::List {
                ordered: true,
                items: r
                    .teaching_script
                    .iter()
                    .map(|s| {
                        vec![
                            Inline {
                                text: format!("{}. ", s.step),
                                bold: true,
                                ..Inline::default()
                            },
                            Inline::plain(s.text.clone()),
                        ]
                    })
                    .collect(),
            },
            Block::Heading {
                level: 2,
                text: "Where to get naloxone".into(),
            },
            Block::List {
                ordered: false,
                items: r
                    .access
                    .iter()
                    .map(|a| vec![Inline::plain(a.text.clone())])
                    .collect(),
            },
            Block::Heading {
                level: 2,
                text: "Help lines".into(),
            },
            Block::List {
                ordered: false,
                items: r
                    .contacts
                    .iter()
                    .map(|c| {
                        vec![
                            Inline {
                                text: format!("{}: ", c.name),
                                bold: true,
                                ..Inline::default()
                            },
                            Inline::plain(c.detail.clone()),
                        ]
                    })
                    .collect(),
            },
            Block::Heading {
                level: 2,
                text: "Sources".into(),
            },
        ];
        let mut keys: Vec<&String> = r
            .teaching_script
            .iter()
            .flat_map(|s| s.sources.iter())
            .collect();
        keys.extend(r.access.iter().map(|a| &a.source));
        keys.sort();
        keys.dedup();
        blocks.push(Block::List {
            ordered: false,
            items: keys
                .iter()
                .filter_map(|k| r.sources.get(*k))
                .map(|s| {
                    vec![Inline {
                        text: format!("{} ({})", s.title, s.publisher),
                        link: s.url.clone(),
                        ..Inline::default()
                    }]
                })
                .collect(),
        });
        blocks.push(Block::Paragraph(vec![Inline::plain(format!(
            "Draft prepared from Massachusetts DPH and CDC sources ({}); pending review by your agency before distribution.",
            r.meta.prepared
        ))]));
        let doc = Document {
            title: "Naloxone: how to stop an opioid overdose".into(),
            subtitle: Some("Massachusetts resources".into()),
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks,
        };
        let bytes = phn_docs::docx::to_docx(&doc)?;
        let sandbox = ctx.sandbox()?;
        let path = sandbox.new_output_file("naloxone_teaching_handout.docx")?;
        std::fs::write(&path, bytes)?;
        let disp = sandbox.display_path(&path);
        summary.push_str(&format!(
            "\nHandout written to `{disp}`. Check it with check_readability before printing.\n"
        ));
        privacy.files_written.push(disp.clone());
        handout_file = Some(disp);
    }

    // ---- sources and caveats
    let mut keys: Vec<&str> = vec![
        "standing_order",
        "pharmacy_policy",
        "pharmacy_page",
        "naloxone_faq",
        "naloxone_locator",
        "ssp_locator",
        "cdc_fentanyl",
        "bsas_contacts",
    ];
    if include_script {
        keys.extend(["oend_competencies", "cdc_naloxone", "cdc_response"]);
    }
    if syringe.is_some() {
        keys.push(r.syringe_services.statute_source.as_str());
    }
    keys.push("mgl_94c_1");
    let sources = content::cite(&r.sources, &keys, &r.meta.prepared);
    let mut caveats = vec![
        "DPH's naloxone program and syringe service program maps could not be read by the server; open the links for current sites, hours, and services.".to_string(),
        "Syringe service approval comes from DPH's list of cities and towns whose board of health approved a program; approval does not mean a program operates there now.".to_string(),
        "The teaching script combines DPH's 2019 OEND core competencies with the 2026 standing order and CDC pages; rescue breathing and CPR steps are for people trained to do them. Follow your agency's protocols and training.".to_string(),
        "The legal status of fentanyl test strips under M.G.L. c. 94C, s. 1 could not be confirmed; check with agency counsel.".to_string(),
    ];
    caveats.extend(content::review_caveats(&[SUBSTANCE_USE]));
    Ok(Envelope::new(
        summary,
        HarmReductionResult {
            town: syringe.as_ref().map(|s| s.town.clone()),
            ways_to_get_naloxone: r.access.clone(),
            standing_order_signed: r.standing_order.signed.clone(),
            standing_order_authority: r.standing_order.authority.clone(),
            standing_order_formulations: r.standing_order.formulations.clone(),
            pharmacy_rules: r.pharmacy_rules.clone(),
            legal_protections: r.legal.clone(),
            syringe_services: syringe,
            fentanyl_test_strips: r.fentanyl_test_strips.clone(),
            overdose_risk_factors: r.risk_factors.clone(),
            teaching_script: if include_script {
                r.teaching_script.clone()
            } else {
                Vec::new()
            },
            contacts: r.contacts.clone(),
            locators,
            handout_file,
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

    #[test]
    fn content_loads_and_every_cited_source_exists() {
        let r = &*RESOURCES;
        assert_eq!(r.standing_order.formulations.len(), 5);
        let mut keys: Vec<&String> = r
            .teaching_script
            .iter()
            .flat_map(|s| s.sources.iter())
            .collect();
        for list in [
            &r.access,
            &r.pharmacy_rules,
            &r.legal,
            &r.fentanyl_test_strips,
            &r.risk_factors,
            &r.treatment,
        ] {
            keys.extend(list.iter().map(|c| &c.source));
        }
        keys.extend(r.contacts.iter().map(|c| &c.source));
        keys.push(&r.syringe_services.statute_source);
        keys.push(&r.syringe_services.list_source);
        for k in keys {
            assert!(r.sources.contains_key(k), "unknown source {k}");
        }
    }

    #[test]
    fn every_approved_ssp_town_is_a_massachusetts_town() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        for t in &RESOURCES.syringe_services.approved_towns {
            assert!(ctx.ma.town(t).is_some(), "{t}");
        }
    }
}
