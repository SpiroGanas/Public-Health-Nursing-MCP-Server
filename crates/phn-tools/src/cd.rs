//! Communicable disease tools.
//!
//! * CD-01 `lookup_reportable_condition`

use phn_content::reportable::{
    self, CONFIDENT_MATCH, Confidence, Contact, ReportTo, Reporter, Timeframe,
};
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;

/// Arguments for `lookup_reportable_condition`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct LookupReportableArgs {
    /// Disease, condition, organism, or laboratory result, e.g. "pertussis",
    /// "Salmonella Typhi", "positive HBsAg", or "two kids vomiting at a daycare".
    pub query: String,
    /// How many other possible matches to list (0–5, default 3).
    #[serde(default)]
    pub max_alternatives: Option<u8>,
}

/// The matched condition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConditionAnswer {
    pub id: String,
    pub name: String,
    /// The name, alias, or lab term that matched the query.
    pub matched_on: String,
    pub reportable: bool,
    pub timeframe: Timeframe,
    pub timeframe_text: String,
    /// Who must report.
    pub reporters: Vec<Reporter>,
    /// Where reports go.
    pub report_to: Vec<ReportTo>,
    pub how_to_report: String,
    /// Chapter of DPH's Guide to Surveillance, Reporting and Control.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guide_chapter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guide_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Developer confidence in this draft entry.
    pub content_confidence: Confidence,
}

/// Another possible match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Alternative {
    pub id: String,
    pub name: String,
    pub timeframe: Timeframe,
    pub score: f64,
}

/// The nurse's own board of health, from settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LocalContact {
    pub town: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board_of_health_phone: Option<String>,
}

/// Result of `lookup_reportable_condition`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReportableLookup {
    pub query: String,
    /// True when the query matched a listed condition confidently.
    pub matched: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<ConditionAnswer>,
    pub alternatives: Vec<Alternative>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_contact: Option<LocalContact>,
    pub dph_epidemiology: Contact,
    /// Reminder that clusters and unusual illnesses are always reportable.
    pub catch_all: String,
}

const CATCH_ALL: &str = "Any cluster or outbreak of illness, and any case of an unusual illness \
thought to have public health implications, is immediately reportable even if the disease is \
not on the list.";

/// Is a disease or lab result reportable in Massachusetts, by whom, how fast, and to whom?
pub fn lookup_reportable_condition(
    ctx: &Context,
    args: LookupReportableArgs,
) -> Result<Envelope<ReportableLookup>> {
    let query = args.query.trim();
    if query.is_empty() {
        return Err(PhnError::InvalidInput("query must not be empty".into()));
    }
    let content = reportable::content();
    let max_alt = usize::from(args.max_alternatives.unwrap_or(3).min(5));
    let hits = reportable::lookup(query);
    let dph = content
        .contacts
        .get("dph_epidemiology")
        .cloned()
        .ok_or_else(|| PhnError::Internal("DPH contact missing from content".into()))?;
    let guide_url = content
        .meta
        .related_sources
        .first()
        .and_then(|s| s.url.clone());
    let settings = ctx.settings();
    let local_contact = settings.jurisdiction.town.clone().map(|town| LocalContact {
        town,
        board_of_health_phone: settings.jurisdiction.board_of_health_phone.clone(),
    });

    let best = hits.first().filter(|h| h.score >= CONFIDENT_MATCH);
    let condition = best.map(|h| {
        let c = h.condition;
        let how = how_to_report(c.timeframe, &c.report_to, &dph);
        ConditionAnswer {
            id: c.id.clone(),
            name: c.name.clone(),
            matched_on: h.matched_on.clone(),
            reportable: true,
            timeframe: c.timeframe,
            timeframe_text: content.timeframes.for_timeframe(c.timeframe).to_string(),
            reporters: c.reporters.clone(),
            report_to: c.report_to.clone(),
            how_to_report: how,
            guide_chapter: c.guide_chapter.clone(),
            guide_url: c.guide_chapter.as_ref().and(guide_url.clone()),
            notes: c.notes.clone(),
            content_confidence: c.confidence,
        }
    });
    let skip = usize::from(condition.is_some());
    let alternatives: Vec<Alternative> = hits
        .iter()
        .skip(skip)
        .filter(|h| condition.as_ref().is_none_or(|c| c.id != h.condition.id))
        .take(max_alt)
        .map(|h| Alternative {
            id: h.condition.id.clone(),
            name: h.condition.name.clone(),
            timeframe: h.condition.timeframe,
            score: (h.score * 100.0).round() / 100.0,
        })
        .collect();

    // Summary.
    let mut summary = String::new();
    let mut caveats = Vec::new();
    match &condition {
        Some(c) => {
            summary.push_str(&format!(
                "**{}** is reportable in Massachusetts under 105 CMR 300.000.\n\n",
                c.name
            ));
            summary.push_str(&format!("- **How fast:** {}\n", c.timeframe.label()));
            summary.push_str(&format!(
                "- **Who reports:** {}\n",
                c.reporters
                    .iter()
                    .map(|r| r.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            summary.push_str(&format!(
                "- **Report to:** {}\n",
                c.report_to
                    .iter()
                    .map(|r| r.label())
                    .collect::<Vec<_>>()
                    .join(" and ")
            ));
            summary.push_str(&format!("- **How:** {}\n", c.how_to_report));
            if let Some(ch) = &c.guide_chapter {
                summary.push_str(&format!(
                    "- **DPH guide chapter:** {ch} (Guide to Surveillance, Reporting and Control)\n"
                ));
            }
            if let Some(n) = &c.notes {
                summary.push_str(&format!("- **Note:** {n}\n"));
            }
            if c.matched_on != c.name {
                summary.push_str(&format!("\nMatched on \"{}\".\n", c.matched_on));
            }
            if c.content_confidence == Confidence::Low {
                caveats.push(format!(
                    "The draft entry for {} is LOW confidence: the reporting timeframe or \
                     reporters may be wrong. Confirm with DPH Epidemiology ({}) before relying on it.",
                    c.name, dph.phone
                ));
            }
        }
        None => {
            summary.push_str(&format!(
                "\"{query}\" did not match a condition on the Massachusetts reportable list \
                 bundled with this server. **This does not mean it is not reportable.**\n\n"
            ));
            summary.push_str(&format!("- {CATCH_ALL}\n"));
            summary.push_str(&format!(
                "- When unsure, call {} at {}.\n",
                dph.name, dph.phone
            ));
        }
    }
    if !alternatives.is_empty() {
        summary.push_str("\nOther possible matches: ");
        summary.push_str(
            &alternatives
                .iter()
                .map(|a| format!("{} ({})", a.name, a.timeframe.label()))
                .collect::<Vec<_>>()
                .join("; "),
        );
        summary.push('\n');
    }
    if let Some(lc) = &local_contact {
        summary.push_str(&format!(
            "\nYour board of health: {}{}.\n",
            lc.town,
            lc.board_of_health_phone
                .as_ref()
                .map(|p| format!(", {p}"))
                .unwrap_or_default()
        ));
    }
    summary.push_str(&format!("\nDPH Epidemiology (24/7): {}.\n", dph.phone));

    if let Some(c) = content.meta.review_caveat() {
        caveats.insert(0, c);
    }
    let mut sources = vec![Source {
        title: content.meta.source.title.clone(),
        publisher: content.meta.source.publisher.clone(),
        url: content.meta.source.url.clone(),
        effective_date: content.meta.effective_date.clone(),
        retrieved: Some(content.meta.prepared.clone()),
        mode: SourceMode::Snapshot,
    }];
    if condition
        .as_ref()
        .is_some_and(|c| c.guide_chapter.is_some())
    {
        if let Some(rel) = content.meta.related_sources.first() {
            sources.push(Source {
                title: rel.title.clone(),
                publisher: rel.publisher.clone(),
                url: rel.url.clone(),
                effective_date: None,
                retrieved: None,
                mode: SourceMode::Snapshot,
            });
        }
    }

    Ok(Envelope::new(
        summary,
        ReportableLookup {
            query: query.to_string(),
            matched: condition.is_some(),
            condition,
            alternatives,
            local_contact,
            dph_epidemiology: dph,
            catch_all: CATCH_ALL.into(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy::none())
    .decision_support())
}

fn how_to_report(t: Timeframe, to: &[ReportTo], dph: &Contact) -> String {
    let to_lboh = to.contains(&ReportTo::LocalBoardOfHealth);
    match (t, to_lboh) {
        (Timeframe::Immediate, true) => format!(
            "Telephone the local board of health of the patient's town of residence now; if it \
             cannot be reached, call DPH Epidemiology at {}. Follow up with a written or MAVEN report.",
            dph.phone
        ),
        (Timeframe::Immediate, false) => format!(
            "Telephone DPH Epidemiology at {} now, then follow up in writing or in MAVEN.",
            dph.phone
        ),
        (Timeframe::Within24Hours, true) => "Report to the local board of health of the \
             patient's town of residence (or to DPH) within 24 hours by phone, fax, or MAVEN. \
             Laboratory results reach the board of health through MAVEN."
            .into(),
        (Timeframe::Within24Hours, false) => {
            "Report to DPH within 24 hours as directed by the DPH program.".into()
        }
        (Timeframe::Other, _) => {
            "Report through the DPH program named in the notes, following its current instructions."
                .into()
        }
    }
}
