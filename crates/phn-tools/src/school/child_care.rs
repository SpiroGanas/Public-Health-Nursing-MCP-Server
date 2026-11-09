//! SCH-08 `child_care_health_consultation_checklist`.
//!
//! A checklist for child care health consultants against the EEC licensing
//! standards (606 CMR 7.00): the health care policy, medication
//! administration, individual health care plans, immunization and other
//! health records, illness exclusion and notification, infection control,
//! first aid and emergencies, safe sleep, food allergies, educator health,
//! and safety. Filtered by program type; optional findings give a compliance
//! summary; an optional worksheet is written to the output folder.

use std::collections::BTreeMap;

use phn_core::{Envelope, PhnError, Privacy, Result};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;
use crate::school::content::{self, CHILD_CARE_INFO, ChecklistItem, CitedOut, outs};

/// EEC program type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProgramType {
    /// Family child care (in the educator's home).
    Family,
    /// Small group and school age child care.
    SmallGroup,
    /// Large group and school age child care (default; most centers).
    #[default]
    LargeGroup,
}

impl ProgramType {
    fn key(self) -> &'static str {
        match self {
            ProgramType::Family => "family",
            ProgramType::SmallGroup => "small_group",
            ProgramType::LargeGroup => "large_group",
        }
    }

    fn label(self) -> &'static str {
        match self {
            ProgramType::Family => "family child care",
            ProgramType::SmallGroup => "small group and school age child care",
            ProgramType::LargeGroup => "large group and school age child care",
        }
    }
}

/// A consultant's finding for one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindingStatus {
    Met,
    Partial,
    NotMet,
    NotApplicable,
}

impl FindingStatus {
    fn label(self) -> &'static str {
        match self {
            FindingStatus::Met => "met",
            FindingStatus::Partial => "partly met",
            FindingStatus::NotMet => "not met",
            FindingStatus::NotApplicable => "not applicable",
        }
    }
}

/// A finding recorded during a consultation visit.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct Finding {
    /// Item id from the checklist (e.g. "med_training").
    pub item: String,
    pub status: FindingStatus,
    /// Optional note (do not include children's names).
    #[serde(default)]
    pub note: Option<String>,
}

/// Arguments for `child_care_health_consultation_checklist`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ChildCareChecklistArgs {
    /// family, small_group, or large_group (default).
    pub program_type: Option<ProgramType>,
    /// Domains to include (default all): health_policy, consultant,
    /// medication, care_plans, records, illness, infection_control,
    /// first_aid, safe_sleep, nutrition, staff_health, safety.
    pub domains: Option<Vec<String>>,
    /// Findings from a visit, to summarize compliance and list gaps.
    pub findings: Option<Vec<Finding>>,
    /// Write a consultation worksheet to the output folder (default false).
    pub write_worksheet: Option<bool>,
    /// Worksheet format: xlsx (default) or docx.
    pub worksheet_format: Option<WorksheetFormat>,
    /// Program name for the worksheet title (a program, not a person).
    pub program_name: Option<String>,
}

/// Worksheet format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorksheetFormat {
    #[default]
    Xlsx,
    Docx,
}

/// One checklist item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemOut {
    pub id: String,
    pub domain: String,
    pub requirement: String,
    pub evidence: Vec<String>,
    pub citation: String,
    pub status: Option<FindingStatus>,
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<content::Confidence>,
}

/// Compliance counts for one domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DomainSummary {
    pub domain: String,
    pub items: usize,
    pub met: usize,
    pub partial: usize,
    pub not_met: usize,
    pub not_applicable: usize,
    pub not_reviewed: usize,
}

/// Result of `child_care_health_consultation_checklist`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChildCareChecklistResult {
    pub program_type: ProgramType,
    pub items: Vec<ItemOut>,
    pub domains: Vec<DomainSummary>,
    /// Items not met or partly met, with citations.
    pub gaps: Vec<ItemOut>,
    pub consultant_role: Vec<CitedOut>,
    pub related: Vec<CitedOut>,
    pub worksheet: Option<String>,
    /// Finding item ids that are not on the checklist.
    pub unknown_items: Vec<String>,
}

fn domain_name(id: &str) -> String {
    CHILD_CARE_INFO
        .domains
        .iter()
        .find(|d| d.id == id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn item_out(i: &ChecklistItem, f: Option<&Finding>) -> ItemOut {
    ItemOut {
        id: i.id.clone(),
        domain: domain_name(&i.domain),
        requirement: i.requirement.trim().to_string(),
        evidence: i.evidence.clone(),
        citation: content::citation(&CHILD_CARE_INFO.sources, &i.source, &i.section),
        status: f.map(|f| f.status),
        note: f.and_then(|f| f.note.clone()),
        confidence: i
            .confidence
            .filter(|c| !matches!(c, content::Confidence::High)),
    }
}

/// Child care health consultation checklist (606 CMR 7.00).
pub fn child_care_health_consultation_checklist(
    ctx: &Context,
    args: ChildCareChecklistArgs,
) -> Result<Envelope<ChildCareChecklistResult>> {
    let c = &*CHILD_CARE_INFO;
    let program = args.program_type.unwrap_or_default();
    let domains: Option<Vec<String>> = match &args.domains {
        Some(v) if !v.is_empty() => {
            let mut out = Vec::new();
            for d in v {
                let n = phn_core::text::normalize(d).replace(' ', "_");
                let found = c
                    .domains
                    .iter()
                    .find(|x| {
                        x.id == n
                            || phn_core::text::normalize(&x.name).replace(' ', "_") == n
                            || x.id.starts_with(&n)
                    })
                    .ok_or_else(|| {
                        PhnError::InvalidInput(format!(
                            "domain '{d}' is not on the checklist; use {}",
                            c.domains
                                .iter()
                                .map(|x| x.id.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ))
                    })?;
                out.push(found.id.clone());
            }
            Some(out)
        }
        _ => None,
    };
    let findings: BTreeMap<String, &Finding> = args
        .findings
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|f| (f.item.trim().to_string(), f))
        .collect();
    let unknown_items: Vec<String> = findings
        .keys()
        .filter(|k| !c.items.iter().any(|i| &i.id == *k))
        .cloned()
        .collect();
    let items: Vec<ItemOut> = c
        .items
        .iter()
        .filter(|i| i.applies_to.iter().any(|a| a == program.key()))
        .filter(|i| domains.as_ref().is_none_or(|d| d.contains(&i.domain)))
        .map(|i| item_out(i, findings.get(&i.id).copied()))
        .collect();
    if items.is_empty() {
        return Err(PhnError::InvalidInput(
            "no checklist items match that program type and domain".into(),
        ));
    }
    let mut summaries: Vec<DomainSummary> = Vec::new();
    for d in &c.domains {
        let in_d: Vec<&ItemOut> = items.iter().filter(|i| i.domain == d.name).collect();
        if in_d.is_empty() {
            continue;
        }
        let count = |s: FindingStatus| in_d.iter().filter(|i| i.status == Some(s)).count();
        summaries.push(DomainSummary {
            domain: d.name.clone(),
            items: in_d.len(),
            met: count(FindingStatus::Met),
            partial: count(FindingStatus::Partial),
            not_met: count(FindingStatus::NotMet),
            not_applicable: count(FindingStatus::NotApplicable),
            not_reviewed: in_d.iter().filter(|i| i.status.is_none()).count(),
        });
    }
    let gaps: Vec<ItemOut> = items
        .iter()
        .filter(|i| {
            matches!(
                i.status,
                Some(FindingStatus::NotMet | FindingStatus::Partial)
            )
        })
        .cloned()
        .collect();

    // ---- Worksheet
    let worksheet = if args.write_worksheet.unwrap_or(false) {
        let title = format!(
            "Child care health consultation: {}",
            args.program_name
                .clone()
                .unwrap_or_else(|| program.label().to_string())
        );
        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|i| {
                vec![
                    i.domain.clone(),
                    i.id.clone(),
                    i.requirement.clone(),
                    i.citation.clone(),
                    i.evidence.join("; "),
                    i.status.map(|s| s.label().to_string()).unwrap_or_default(),
                    i.note.clone().unwrap_or_default(),
                ]
            })
            .collect();
        let doc = Document {
            title: title.clone(),
            subtitle: Some(format!("606 CMR 7.00, {}", program.label())),
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks: vec![
                Block::Paragraph(vec![Inline::plain(
                    "Checklist drafted from 606 CMR 7.00 (Mass. Register #1459, 12/24/21); pending \
                     review. Record the status and notes during the visit; do not record \
                     children's names.",
                )]),
                Block::Table(TableBlock {
                    caption: Some("Checklist".into()),
                    headers: [
                        "Domain",
                        "Item",
                        "Requirement",
                        "Citation",
                        "Evidence to look for",
                        "Status",
                        "Notes",
                    ]
                    .map(String::from)
                    .to_vec(),
                    rows,
                }),
            ],
        };
        let (bytes, ext) = match args.worksheet_format.unwrap_or_default() {
            WorksheetFormat::Xlsx => (phn_docs::xlsx::to_xlsx(&doc)?, "xlsx"),
            WorksheetFormat::Docx => (phn_docs::docx::to_docx(&doc)?, "docx"),
        };
        let sandbox = ctx.sandbox()?;
        let out = sandbox.new_output_file(&format!("child_care_health_checklist.{ext}"))?;
        std::fs::write(&out, bytes)?;
        Some(sandbox.display_path(&out))
    } else {
        None
    };

    // ---- Markdown
    let mut s = format!(
        "**Child care health consultation checklist — {} (606 CMR 7.00)**\n\n",
        program.label()
    );
    if !findings.is_empty() {
        let rows: Vec<Vec<String>> = summaries
            .iter()
            .map(|d| {
                vec![
                    d.domain.clone(),
                    d.items.to_string(),
                    d.met.to_string(),
                    d.partial.to_string(),
                    d.not_met.to_string(),
                    d.not_reviewed.to_string(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "Domain",
                "Items",
                "Met",
                "Partly",
                "Not met",
                "Not reviewed",
            ],
            &rows,
        ));
        if !gaps.is_empty() {
            s.push_str("\n**Gaps to address**\n\n");
            for g in &gaps {
                s.push_str(&format!(
                    "- [{}] {} ({}){}\n",
                    g.status.map(|x| x.label()).unwrap_or_default(),
                    g.requirement,
                    g.citation,
                    g.note
                        .as_deref()
                        .map(|n| format!(" — note: {n}"))
                        .unwrap_or_default()
                ));
            }
        }
        s.push('\n');
    }
    let mut current = String::new();
    for i in &items {
        if i.domain != current {
            current = i.domain.clone();
            s.push_str(&format!("\n**{current}**\n\n"));
        }
        s.push_str(&format!(
            "- `{}` {} ({}){}{}\n  Evidence: {}\n",
            i.id,
            i.requirement,
            i.citation,
            i.status
                .map(|x| format!(" — **{}**", x.label()))
                .unwrap_or_default(),
            content::confidence_suffix(i.confidence),
            i.evidence.join("; ")
        ));
    }
    let consultant_role = outs(&c.consultant_role, &c.sources);
    s.push_str("\n**The health care consultant's role**\n\n");
    for r in &consultant_role {
        s.push_str(&format!("- {} ({})\n", r.text, r.citation));
    }
    if let Some(w) = &worksheet {
        s.push_str(&format!("\nWorksheet written to `{w}`.\n"));
    }
    if !unknown_items.is_empty() {
        s.push_str(&format!(
            "\nFindings for items not on this checklist were ignored: {}.\n",
            unknown_items.join(", ")
        ));
    }
    let mut caveats = content::review(&[content::CHILD_CARE]);
    caveats.push(
        "This checklist covers health-related licensing standards only; EEC's full regulations, \
         policies, and technical assistance also apply. Immunization requirements for child care \
         come from 105 CMR 220.000 (see check_ma_school_immunization_requirements), and illness \
         exclusions from DPH (see calculate_isolation_and_exclusion)."
            .into(),
    );
    let mut privacy = Privacy::none();
    if let Some(w) = &worksheet {
        privacy.files_written.push(w.clone());
    }
    Ok(Envelope::new(
        s,
        ChildCareChecklistResult {
            program_type: program,
            items,
            domains: summaries,
            gaps,
            consultant_role,
            related: outs(&c.related, &c.sources),
            worksheet,
            unknown_items,
        },
    )
    .sources(content::sources_for(&c.sources, ["cmr606-7"]))
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
    fn program_types_filter_items() {
        let (_d, c) = ctx();
        let large = child_care_health_consultation_checklist(&c, ChildCareChecklistArgs::default())
            .unwrap();
        let family = child_care_health_consultation_checklist(
            &c,
            ChildCareChecklistArgs {
                program_type: Some(ProgramType::Family),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(large.data.items.len() > family.data.items.len());
        assert!(
            large
                .data
                .items
                .iter()
                .any(|i| i.id == "consultant_approvals")
        );
        assert!(
            !family
                .data
                .items
                .iter()
                .any(|i| i.id == "consultant_approvals")
        );
        assert!(
            large
                .data
                .items
                .iter()
                .all(|i| i.citation.contains("606 CMR 7.00"))
        );
    }

    #[test]
    fn findings_give_gaps_and_a_worksheet() {
        let (d, c) = ctx();
        let e = child_care_health_consultation_checklist(
            &c,
            ChildCareChecklistArgs {
                domains: Some(vec!["medication".into()]),
                findings: Some(vec![
                    Finding {
                        item: "med_training".into(),
                        status: FindingStatus::NotMet,
                        note: Some("no competency records".into()),
                    },
                    Finding {
                        item: "med_policy".into(),
                        status: FindingStatus::Met,
                        note: None,
                    },
                    Finding {
                        item: "zzz".into(),
                        status: FindingStatus::Met,
                        note: None,
                    },
                ]),
                write_worksheet: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(e.data.gaps.len(), 1);
        assert_eq!(e.data.domains.len(), 1);
        assert_eq!(e.data.domains[0].met, 1);
        assert_eq!(e.data.unknown_items, vec!["zzz".to_string()]);
        assert!(
            d.path()
                .join("data")
                .join(e.data.worksheet.as_ref().unwrap())
                .exists()
        );
        assert!(
            child_care_health_consultation_checklist(
                &c,
                ChildCareChecklistArgs {
                    domains: Some(vec!["parking".into()]),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
