//! CD-13 `infection_control_for_congregate_settings`.

use phn_content::infection_control::{self, CongregateSetting, Note};
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// Arguments for `infection_control_for_congregate_settings`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct InfectionControlArgs {
    /// Pathogen or condition, e.g. "norovirus", "C. diff", "scabies", "flu", "MRSA".
    pub pathogen: String,
    /// The setting: long_term_care, school, child_care, shelter, or day_program
    /// (default: guidance for all of them).
    #[serde(default)]
    pub setting: Option<CongregateSetting>,
}

/// A precaution type with its definition and PPE.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PrecautionOut {
    pub label: String,
    pub definition: String,
    pub ppe: Vec<String>,
    pub citation: String,
}

/// An EPA disinfectant list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EpaListOut {
    pub list: String,
    pub title: String,
    pub url: String,
}

/// A cited note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NoteOut {
    pub text: String,
    pub citation: String,
}

/// Guidance for one setting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SettingGuidance {
    pub setting: CongregateSetting,
    pub label: String,
    /// True when the setting is a health care setting (Appendix A applies directly).
    pub health_care_setting: bool,
    pub notes: Vec<NoteOut>,
}

/// Result of `infection_control_for_congregate_settings`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InfectionControlResult {
    pub pathogen: String,
    /// Other pathogens that also matched the query.
    pub other_matches: Vec<String>,
    /// Transmission-based precautions (CDC/HICPAC Appendix A).
    pub precautions: String,
    pub precaution_types: Vec<PrecautionOut>,
    pub duration: String,
    /// Appendix A comments (including cohorting where stated).
    pub comments: String,
    pub hand_hygiene: String,
    /// EPA lists of registered disinfectants with claims against this pathogen.
    pub epa_lists: Vec<EpaListOut>,
    /// Guidance that applies in every setting.
    pub general_notes: Vec<NoteOut>,
    pub settings: Vec<SettingGuidance>,
    pub disinfectant_basics: Vec<NoteOut>,
    pub citation: String,
}

fn cite(source: &str, section: &str) -> String {
    let c = infection_control::content();
    let title = c
        .sources
        .get(source)
        .map(|s| s.title.clone())
        .unwrap_or_else(|| source.to_string());
    format!("{title}, {section}")
}

fn notes(v: &[Note]) -> Vec<NoteOut> {
    v.iter()
        .map(|n| NoteOut {
            text: n.text.clone(),
            citation: cite(&n.source, &n.section),
        })
        .collect()
}

/// Infection control guidance for a pathogen in congregate settings.
pub fn infection_control_for_congregate_settings(
    _ctx: &Context,
    args: InfectionControlArgs,
) -> Result<Envelope<InfectionControlResult>> {
    let c = infection_control::content();
    let q = args.pathogen.trim();
    if q.is_empty() {
        return Err(PhnError::InvalidInput(
            "give a pathogen or condition".into(),
        ));
    }
    let matches = infection_control::find(q);
    // Only an exact name or a close match counts; a pathogen that is not
    // covered must not borrow the guidance of one with a similar spelling.
    let Some(p) = infection_control::best_match(q) else {
        let names: Vec<&str> = c.pathogens.iter().map(|p| p.name.as_str()).collect();
        return Err(PhnError::NotFound(format!(
            "no infection control guidance matches '{q}' (this tool does not guess from similar \
             names). Covered: {}. For other organisms, see CDC/HICPAC Appendix A ({}).",
            names.join("; "),
            c.sources
                .get("cdc-appendix-a")
                .map(|s| s.url.as_str())
                .unwrap_or_default()
        )));
    };
    let other_matches: Vec<String> = matches
        .iter()
        .skip(1)
        .filter(|(o, s)| *s >= 0.9 && o.id != p.id)
        .map(|(o, _)| o.name.clone())
        .collect();

    let lower = p.precautions.to_lowercase();
    let precaution_types: Vec<PrecautionOut> = c
        .precaution_types
        .iter()
        .filter(|t| lower.contains(&t.id))
        .map(|t| PrecautionOut {
            label: t.label.clone(),
            definition: t.definition.clone(),
            ppe: t.ppe.clone(),
            citation: cite(&t.source, &t.section),
        })
        .collect();
    let epa_lists: Vec<EpaListOut> = p
        .epa_lists
        .iter()
        .filter_map(|id| c.epa_lists.iter().find(|l| &l.id == id))
        .map(|l| EpaListOut {
            list: l.id.clone(),
            title: l.title.clone(),
            url: l.url.clone(),
        })
        .collect();
    let general_notes = p
        .setting_notes
        .get("all")
        .map(|v| notes(v))
        .unwrap_or_default();
    let wanted: Vec<CongregateSetting> = match args.setting {
        Some(s) => vec![s],
        None => c.settings.iter().map(|s| s.id).collect(),
    };
    let settings: Vec<SettingGuidance> = wanted
        .iter()
        .filter_map(|w| c.settings.iter().find(|s| s.id == *w))
        .map(|s| {
            let mut n = notes(&s.notes);
            if let Some(extra) = p.setting_notes.get(s.id.key()) {
                n.extend(notes(extra));
            }
            SettingGuidance {
                setting: s.id,
                label: s.label.clone(),
                health_care_setting: s.health_care_setting,
                notes: n,
            }
        })
        .collect();
    let basics = notes(&c.disinfectant_basics);
    let citation = cite(&p.source, &p.section);

    // ---- Markdown
    let mut s = format!(
        "**Infection control: {}**{}\n\n",
        p.name,
        args.setting
            .and_then(|w| c.settings.iter().find(|s| s.id == w))
            .map(|s| format!(" — {}", s.label))
            .unwrap_or_default()
    );
    s.push_str(&md::table(
        &["", "Guidance"],
        &[
            vec!["Precautions".into(), p.precautions.clone()],
            vec!["Duration".into(), p.duration.clone()],
            vec!["Hand hygiene".into(), p.hand_hygiene.clone()],
            vec![
                "Comments (incl. cohorting)".into(),
                p.appendix_a_comment.clone(),
            ],
        ],
    ));
    s.push_str(&format!("\n_Source: {citation}_\n"));
    if !precaution_types.is_empty() {
        s.push_str("\n**What the precautions mean (105 CMR 300.020)**\n\n");
        for t in &precaution_types {
            s.push_str(&format!(
                "- **{}**: {} PPE: {}.\n",
                t.label,
                t.definition,
                t.ppe.join(", ")
            ));
        }
    }
    s.push_str("\n**Disinfectant selection**\n\n");
    if epa_lists.is_empty() {
        s.push_str(
            "No pathogen-specific EPA list is cited for this organism; use an EPA-registered \
             disinfectant labeled for the use site and the organism, following the label's \
             contact time.\n",
        );
    } else {
        for l in &epa_lists {
            s.push_str(&format!("- {} — {}\n", l.title, l.url));
        }
    }
    for b in &basics {
        s.push_str(&format!("- {} ({})\n", b.text, b.citation));
    }
    if !general_notes.is_empty() {
        s.push_str("\n**All settings**\n\n");
        for n in &general_notes {
            s.push_str(&format!("- {} ({})\n", n.text, n.citation));
        }
    }
    for g in &settings {
        s.push_str(&format!(
            "\n**{}**{}\n\n",
            g.label,
            if g.health_care_setting {
                " (health care setting)"
            } else {
                " (not a health care setting: general-population guidance applies)"
            }
        ));
        for n in &g.notes {
            s.push_str(&format!("- {} ({})\n", n.text, n.citation));
        }
    }
    if !other_matches.is_empty() {
        s.push_str(&format!(
            "\nAlso matched: {} — ask for one of these if it was meant.\n",
            other_matches.join("; ")
        ));
    }

    // ---- Sources and caveats
    let mut ids: Vec<String> = vec![p.source.clone(), "cmr300".into()];
    if !epa_lists.is_empty() || !basics.is_empty() {
        ids.push("epa-disinfectants".into());
    }
    for n in p.setting_notes.values().flatten() {
        ids.push(n.source.clone());
    }
    for g in &settings {
        if let Some(si) = c.settings.iter().find(|s| s.id == g.setting) {
            ids.extend(si.notes.iter().map(|n| n.source.clone()));
        }
    }
    ids.sort();
    ids.dedup();
    let sources: Vec<Source> = ids
        .iter()
        .filter_map(|id| c.sources.get(id))
        .map(|s| Source {
            title: s.title.clone(),
            publisher: s.publisher.clone(),
            url: Some(s.url.clone()),
            effective_date: s.effective_date.clone(),
            retrieved: Some(s.retrieved.clone()),
            mode: SourceMode::Snapshot,
        })
        .collect();
    let mut caveats = vec![format!(
        "REVIEW STATUS: the infection control content is a PENDING-REVIEW draft (version {}, \
         prepared {}); setting notes are summaries. The nurse must review it before Pilot 1 \
         (review due {}).",
        c.meta.version, c.meta.prepared, c.meta.review.review_due
    )];
    if p.confidence == Some(phn_content::reportable::Confidence::Medium)
        || p.confidence == Some(phn_content::reportable::Confidence::Low)
    {
        caveats.push(format!(
            "This entry is {} confidence; confirm with DPH.",
            match p.confidence {
                Some(phn_content::reportable::Confidence::Low) => "LOW",
                _ => "medium",
            }
        ));
    }
    caveats.push(
        "EPA lists change often and are not all-inclusive: open the linked list, match the \
         product by its EPA registration number, and follow the label (use site and contact time)."
            .into(),
    );
    caveats.push(
        "Appendix A is written for health care settings. Shelters, schools, child care, and day \
         programs that are not health care settings follow DPH general-population guidance; \
         adapt precautions with DPH and the local board of health."
            .into(),
    );
    let data = InfectionControlResult {
        pathogen: p.name.clone(),
        other_matches,
        precautions: p.precautions.clone(),
        precaution_types,
        duration: p.duration.clone(),
        comments: p.appendix_a_comment.clone(),
        hand_hygiene: p.hand_hygiene.clone(),
        epa_lists,
        general_notes,
        settings,
        disinfectant_basics: basics,
        citation,
    };
    Ok(Envelope::new(s, data)
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

    #[test]
    fn norovirus_in_long_term_care() {
        let (_d, c) = ctx();
        let env = infection_control_for_congregate_settings(
            &c,
            InfectionControlArgs {
                pathogen: "norovirus".into(),
                setting: Some(CongregateSetting::LongTermCare),
            },
        )
        .unwrap();
        let d = &env.data;
        assert_eq!(d.epa_lists[0].list, "G");
        assert!(
            d.precaution_types
                .iter()
                .any(|t| t.label.contains("Contact"))
        );
        assert_eq!(d.settings.len(), 1);
        assert!(
            d.settings[0]
                .notes
                .iter()
                .any(|n| n.text.contains("Cohort"))
        );
        assert!(env.summary.contains("List G"));
        assert!(env.notice.is_some());
    }

    #[test]
    fn uncovered_pathogens_are_not_found_instead_of_matched_loosely() {
        let (_d, c) = ctx();
        for q in ["ebola", "CRE", "impetigo", "bed bugs", "legionella"] {
            let e = infection_control_for_congregate_settings(
                &c,
                InfectionControlArgs {
                    pathogen: q.into(),
                    setting: None,
                },
            )
            .unwrap_err();
            assert!(e.to_string().contains("Covered"), "{q}: {e}");
        }
        let env = infection_control_for_congregate_settings(
            &c,
            InfectionControlArgs {
                pathogen: "MRSA".into(),
                setting: None,
            },
        )
        .unwrap();
        assert!(env.data.pathogen.contains("MRSA"));
    }

    #[test]
    fn unknown_pathogen_lists_coverage() {
        let (_d, c) = ctx();
        let e = infection_control_for_congregate_settings(
            &c,
            InfectionControlArgs {
                pathogen: "qqqqqq".into(),
                setting: None,
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("Covered"));
    }
}
