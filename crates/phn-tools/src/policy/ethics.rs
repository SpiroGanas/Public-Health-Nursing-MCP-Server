//! POL-05 `public_health_ethics_analysis`: structures an ethics analysis of a
//! public health decision (quarantine or isolation, exclusion from school,
//! allocation of scarce resources, other actions) with the Kass framework and
//! the APHA Public Health Code of Ethics (2019). The nurse supplies the facts;
//! the tool supplies the structured questions, a table of options from least
//! to most restrictive, a justification checklist, and a worksheet (DOCX in
//! /data/output). It never recommends a decision. Source texts are paraphrased
//! and cited, with their copyright noted.

use std::collections::BTreeMap;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::content::{self, Cited, CitedOut, Confidence, DocSource};
use crate::md;
use crate::policy::shared::{contains_phrase, optional_text, stem_words, write_docx};

/// Content id of `content/policy/public_health_ethics.yaml`.
pub const ETHICS: &str = "policy-public-health-ethics";

const MAX_DECISION: usize = 300;
const MAX_SITUATION: usize = 1500;
const MAX_RESPONSE: usize = 800;
const MAX_OPTIONS: usize = 12;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnreadSource {
    pub title: String,
    pub url: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KassQuestionDef {
    pub number: u8,
    pub id: String,
    pub topic: String,
    pub question: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
    pub prompts: Vec<String>,
    pub facts_to_gather: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BurdenDef {
    pub id: String,
    pub label: String,
    pub note: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JustificationDef {
    pub id: String,
    pub label: String,
    pub question: String,
    pub text: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueDef {
    pub id: String,
    pub label: String,
    pub prompt: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionDef {
    pub level: u8,
    pub option: String,
    pub burden_prompts: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileDef {
    pub id: String,
    pub label: String,
    pub keywords: Vec<String>,
    pub tensions: Vec<String>,
    pub facts_to_gather: Vec<String>,
    pub options: Vec<OptionDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocSectionDef {
    pub heading: String,
    pub prompts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocDef {
    pub title: String,
    pub filename: String,
    pub intro: String,
    pub sections_before: Vec<DocSectionDef>,
    pub sections_after: Vec<DocSectionDef>,
}

/// `content/policy/public_health_ethics.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EthicsContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub unread_sources: Vec<UnreadSource>,
    pub process_steps: Vec<Cited>,
    pub kass_questions: Vec<KassQuestionDef>,
    pub burden_categories: Vec<BurdenDef>,
    pub balancing_criteria: Vec<Cited>,
    pub justification_conditions: Vec<JustificationDef>,
    pub apha_values: Vec<ValueDef>,
    pub apha_analysis_steps: Vec<Cited>,
    pub decision_profiles: Vec<ProfileDef>,
    pub profile_origin_note: String,
    pub copyright_notes: Vec<String>,
    pub document: DocDef,
}

// ---------------------------------------------------------------- arguments and results

/// Your response to one of the six framework questions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct KassResponse {
    /// The question number (1 to 6) or id: goals, effectiveness, burdens, minimize_burdens,
    /// fairness, balance.
    pub question: String,
    /// Your facts and reasoning for this question, in a few sentences. Describe groups and the
    /// situation, never individual people.
    pub response: String,
}

/// Arguments for `public_health_ethics_analysis`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct EthicsArgs {
    /// The kind of decision: "quarantine", "isolation", "exclusion from school", "school
    /// exclusion", "allocation of scarce resources" (vaccine, antivirals, shelter space), or
    /// another action in your own words. Picks the matching profile; others get a general profile.
    pub decision_type: Option<String>,
    /// The decision to be made, in one or two sentences ("whether to exclude unimmunized
    /// students during a measles outbreak"). No names of people.
    pub decision: Option<String>,
    /// A short description of the situation and facts you have (disease, setting, numbers,
    /// timeline). Describe groups, not individual people.
    pub situation: Option<String>,
    /// Your responses to the six framework questions. The tool lists the questions you have not
    /// answered; it never answers them.
    pub responses: Vec<KassResponse>,
    /// Options you are considering, in any order (for example "exclude only unimmunized
    /// students", "voluntary home isolation with support"). They are added to the least
    /// restrictive alternative table for you to rank and assess.
    pub alternatives: Vec<String>,
    /// Write the analysis worksheet (DOCX) to /data/output (default true).
    pub write_document: Option<bool>,
}

/// A Kass question with the prompts and your response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct KassQuestionOut {
    pub number: u8,
    pub id: String,
    pub topic: String,
    pub question: String,
    /// Prompts to work through (this project's drafting aid, derived from the framework).
    pub prompts: Vec<String>,
    pub facts_to_gather: Vec<String>,
    /// Your response, as you gave it.
    pub response: Option<String>,
    /// "answered" or "open".
    pub status: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// The decision profile used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileOut {
    pub id: String,
    pub label: String,
    /// True when decision_type matched a specific profile.
    pub matched: bool,
    pub tensions: Vec<String>,
    pub facts_to_gather: Vec<String>,
    pub origin_note: String,
}

/// One row of the least-restrictive-alternative table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptionRow {
    /// 1 is least restrictive. Your own options have no level until you rank them.
    pub level: Option<u8>,
    pub option: String,
    /// "drafting aid" (this project's illustrative option) or "your option".
    pub origin: String,
    /// What to check about the burden of this option.
    pub burden_prompts: String,
    /// Columns for you to fill in.
    pub to_assess: Vec<String>,
}

/// A checklist item with its source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChecklistItem {
    pub id: String,
    pub label: String,
    pub question: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// An APHA core value with a prompt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ValueOut {
    pub label: String,
    pub prompt: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// Result of `public_health_ethics_analysis`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EthicsResult {
    pub decision: Option<String>,
    pub profile: ProfileOut,
    /// CDC's three functions of public health ethics inquiry.
    pub process: Vec<CitedOut>,
    pub kass_questions: Vec<KassQuestionOut>,
    /// Numbers of the Kass questions with no response yet.
    pub open_questions: Vec<u8>,
    pub burden_categories: Vec<CitedOut>,
    pub balancing_criteria: Vec<CitedOut>,
    /// Options from least to most restrictive, then your own.
    pub least_restrictive_table: Vec<OptionRow>,
    pub justification_checklist: Vec<ChecklistItem>,
    pub apha_core_values: Vec<ValueOut>,
    pub apha_analysis_steps: Vec<CitedOut>,
    pub document_file: Option<String>,
    /// States that the tool does not recommend a decision.
    pub no_recommendation: String,
    pub copyright_notes: Vec<String>,
    /// Sources that could not be read.
    pub sources_not_read: Vec<String>,
}

// ---------------------------------------------------------------- logic

fn cite_title(sources: &BTreeMap<String, DocSource>, key: &str) -> String {
    sources
        .get(key)
        .map(|s| s.title.clone())
        .unwrap_or_else(|| key.to_string())
}

/// Pick the decision profile for `decision_type` (and `decision` text).
pub fn match_profile<'a>(c: &'a EthicsContent, text: &str) -> (&'a ProfileDef, bool) {
    let words = stem_words(text);
    let mut best: Option<(&ProfileDef, usize)> = None;
    for p in &c.decision_profiles {
        if p.id == "general" {
            continue;
        }
        let mut score = 0;
        if contains_phrase(&words, &p.id.replace('_', " ")) {
            score += 3;
        }
        for k in &p.keywords {
            if contains_phrase(&words, k) {
                score += 1;
            }
        }
        if score > 0 && best.is_none_or(|(_, s)| score > s) {
            best = Some((p, score));
        }
    }
    match best {
        Some((p, _)) => (p, true),
        None => (
            c.decision_profiles
                .iter()
                .find(|p| p.id == "general")
                .unwrap_or(&c.decision_profiles[0]),
            false,
        ),
    }
}

fn question_index(c: &EthicsContent, key: &str) -> Option<usize> {
    let k = key.trim().to_lowercase().replace([' ', '-'], "_");
    let n: Option<u8> = k.trim_start_matches('q').parse().ok();
    c.kass_questions
        .iter()
        .position(|q| q.id == k || Some(q.number) == n)
}

fn flatten(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn document_markdown(
    c: &EthicsContent,
    decision: &Option<String>,
    situation: &Option<String>,
    profile: &ProfileOut,
    kass: &[KassQuestionOut],
    table: &[OptionRow],
    checklist: &[ChecklistItem],
    values: &[ValueOut],
) -> String {
    let d = &c.document;
    let fill = |s: &str| -> String {
        let mut out = s.to_string();
        if let Some(x) = decision {
            out = out.replace("[NURSE: the action under consideration, in one sentence]", x);
        }
        out
    };
    let mut m = format!("# {}\n\n{}\n\n", d.title, flatten(&d.intro));
    m.push_str(&format!("Decision profile: {}.\n\n", profile.label));
    for s in &d.sections_before {
        m.push_str(&format!("## {}\n\n", s.heading));
        for p in &s.prompts {
            m.push_str(&format!("- {}\n", fill(p)));
        }
        if s.heading.contains("Facts")
            && let Some(sit) = situation
        {
            m.push_str(&format!("- Situation as described by the preparer: {sit}\n"));
        }
        if s.heading.contains("Facts") {
            for f in &profile.facts_to_gather {
                m.push_str(&format!("- Facts to gather: {f}\n"));
            }
        }
        m.push('\n');
    }
    m.push_str("## 3. Kass framework: six questions\n\n");
    for q in kass {
        m.push_str(&format!("### Question {}: {}\n\n{}\n\n", q.number, q.topic, q.question));
        for p in &q.prompts {
            m.push_str(&format!("- Prompt: {p}\n"));
        }
        m.push_str(&format!(
            "- Response: {}\n\n",
            q.response
                .clone()
                .unwrap_or_else(|| "[NURSE: facts and reasoning for this question]".into())
        ));
    }
    m.push_str("## 4. Least restrictive alternatives\n\n");
    m.push_str(
        "Options from least to most restrictive. The first rows are drafting aids, not a list of \
         what is legally available; edit them. Rank your own options and fill in each column.\n\n",
    );
    let rows: Vec<Vec<String>> = table
        .iter()
        .map(|r| {
            let mut v = vec![
                r.level.map(|l| l.to_string()).unwrap_or_else(|| "[NURSE: rank]".into()),
                r.option.clone(),
                r.burden_prompts.clone(),
            ];
            v.extend(r.to_assess.iter().map(|_| "[NURSE: assess]".to_string()));
            v
        })
        .collect();
    m.push_str(&md::table(
        &[
            "Level",
            "Option",
            "Burdens to check",
            "Liberty and self-determination",
            "Privacy and confidentiality",
            "Justice (who bears the burden)",
            "Evidence it would work",
        ],
        &rows,
    ));
    m.push_str("\n## 5. Justification checklist\n\n");
    let rows: Vec<Vec<String>> = checklist
        .iter()
        .map(|x| {
            vec![
                x.label.clone(),
                x.question.clone(),
                "[NURSE: yes, no, or open; why]".into(),
                x.citation.clone(),
            ]
        })
        .collect();
    m.push_str(&md::table(&["Condition", "Question", "Your assessment", "Source"], &rows));
    m.push_str("\n## 6. APHA core values\n\n");
    let rows: Vec<Vec<String>> = values
        .iter()
        .map(|v| vec![v.label.clone(), v.prompt.clone(), "[NURSE: how the action supports or limits this value]".into()])
        .collect();
    m.push_str(&md::table(&["Value", "Prompt", "Your assessment"], &rows));
    m.push('\n');
    for s in &d.sections_after {
        m.push_str(&format!("## {}\n\n", s.heading));
        for p in &s.prompts {
            m.push_str(&format!("- {p}\n"));
        }
        m.push('\n');
    }
    m.push_str("## Sources and copyright\n\n");
    for n in &c.copyright_notes {
        m.push_str(&format!("- {n}\n"));
    }
    for k in ["kass2001", "overview2010", "apha_values_hpp", "apha_steps_jphp", "cdc_phe"] {
        m.push_str(&format!("- {}\n", cite_title(&c.sources, k)));
    }
    m
}

/// Run the analysis.
pub fn public_health_ethics_analysis(
    ctx: &Context,
    args: EthicsArgs,
) -> Result<Envelope<EthicsResult>> {
    let c: EthicsContent = content::load(ETHICS)?;
    let decision_type = optional_text("decision_type", &args.decision_type, MAX_DECISION)?;
    let decision = optional_text("decision", &args.decision, MAX_DECISION)?;
    let situation = optional_text("situation", &args.situation, MAX_SITUATION)?;
    if args.alternatives.len() > MAX_OPTIONS {
        return Err(PhnError::InvalidInput(format!(
            "give at most {MAX_OPTIONS} alternatives"
        )));
    }
    let mut alternatives = Vec::new();
    for a in &args.alternatives {
        if let Some(t) = optional_text("alternatives", &Some(a.clone()), MAX_DECISION)? {
            alternatives.push(t);
        }
    }

    // Responses to the Kass questions.
    let mut responses: Vec<Option<String>> = vec![None; c.kass_questions.len()];
    for r in &args.responses {
        let Some(i) = question_index(&c, &r.question) else {
            return Err(PhnError::InvalidInput(format!(
                "'{}' is not one of the six questions. Use 1 to 6 or: {}",
                r.question.chars().take(60).collect::<String>(),
                c.kass_questions
                    .iter()
                    .map(|q| q.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        };
        if responses[i].is_some() {
            return Err(PhnError::InvalidInput(format!(
                "question {} has more than one response",
                c.kass_questions[i].number
            )));
        }
        responses[i] = optional_text("response", &Some(r.response.clone()), MAX_RESPONSE)?;
    }

    // Profile.
    let match_text = [decision_type.clone(), decision.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    let (p, matched) = match_profile(&c, &match_text);
    let profile = ProfileOut {
        id: p.id.clone(),
        label: p.label.clone(),
        matched,
        tensions: p.tensions.clone(),
        facts_to_gather: p.facts_to_gather.clone(),
        origin_note: flatten(&c.profile_origin_note),
    };

    let kass: Vec<KassQuestionOut> = c
        .kass_questions
        .iter()
        .zip(&responses)
        .map(|(q, r)| KassQuestionOut {
            number: q.number,
            id: q.id.clone(),
            topic: q.topic.clone(),
            question: q.question.clone(),
            prompts: q.prompts.clone(),
            facts_to_gather: q.facts_to_gather.clone(),
            response: r.clone(),
            status: if r.is_some() { "answered" } else { "open" }.into(),
            citation: format!("{}, {}", cite_title(&c.sources, &q.source), q.section),
            confidence: q.confidence,
        })
        .collect();
    let open_questions: Vec<u8> = kass
        .iter()
        .filter(|q| q.response.is_none())
        .map(|q| q.number)
        .collect();

    // Least restrictive alternatives: profile options, then the nurse's.
    let to_assess = vec![
        "Liberty and self-determination".to_string(),
        "Privacy and confidentiality".to_string(),
        "Justice (who bears the burden)".to_string(),
        "Evidence it would work".to_string(),
    ];
    let mut table: Vec<OptionRow> = p
        .options
        .iter()
        .map(|o| OptionRow {
            level: Some(o.level),
            option: o.option.clone(),
            origin: "drafting aid".into(),
            burden_prompts: o.burden_prompts.clone(),
            to_assess: to_assess.clone(),
        })
        .collect();
    table.sort_by_key(|r| r.level);
    for a in alternatives {
        table.push(OptionRow {
            level: None,
            option: a,
            origin: "your option".into(),
            burden_prompts: "Check each burden category: liberty and self-determination, privacy and confidentiality, and justice."
                .into(),
            to_assess: to_assess.clone(),
        });
    }

    let checklist: Vec<ChecklistItem> = c
        .justification_conditions
        .iter()
        .map(|j| ChecklistItem {
            id: j.id.clone(),
            label: j.label.clone(),
            question: j.question.clone(),
            citation: format!("{}, {}", cite_title(&c.sources, &j.source), j.section),
            confidence: j.confidence,
        })
        .collect();
    let values: Vec<ValueOut> = c
        .apha_values
        .iter()
        .map(|v| ValueOut {
            label: v.label.clone(),
            prompt: v.prompt.clone(),
            citation: format!("{}, {}", cite_title(&c.sources, &v.source), v.section),
            confidence: v.confidence,
        })
        .collect();
    let burden_categories: Vec<CitedOut> = c
        .burden_categories
        .iter()
        .map(|b| CitedOut {
            text: format!("{}: {}", b.label, b.note),
            citation: format!("{}, {}", cite_title(&c.sources, &b.source), b.section),
            confidence: b.confidence,
        })
        .collect();
    let balancing = content::cite_all(&c.balancing_criteria, &c.sources);
    let process = content::cite_all(&c.process_steps, &c.sources);
    let apha_steps = content::cite_all(&c.apha_analysis_steps, &c.sources);

    let mut files_written = Vec::new();
    let mut document_file = None;
    if args.write_document.unwrap_or(true) {
        let md = document_markdown(&c, &decision, &situation, &profile, &kass, &table, &checklist, &values);
        let f = write_docx(ctx, &c.document.filename, &c.document.title, &md)?;
        files_written.push(f.clone());
        document_file = Some(f);
    }

    let no_rec = "This tool structures the analysis and does not recommend a decision. The decision \
                  belongs to the authorized decision-maker, who must also check the legal authority \
                  and process, which are not covered here."
        .to_string();
    let sources_not_read: Vec<String> = c
        .unread_sources
        .iter()
        .map(|u| format!("{} ({})", u.title, u.reason))
        .collect();

    let summary = summary_text(
        &decision, &profile, matched, &decision_type, &kass, &table, &checklist, &document_file, &no_rec,
    );

    let mut keys: Vec<&str> = vec!["kass2001", "overview2010", "apha_values_hpp", "apha_values_emph", "apha_steps_jphp", "cdc_phe"];
    keys.push("project");
    let sources = content::sources_for(keys, &c.sources);

    let mut caveats = vec![no_rec.clone()];
    if !matched && decision_type.is_some() {
        caveats.push(format!(
            "No specific profile matched the decision type; the general profile is used. Profiles \
             exist for: {}.",
            c.decision_profiles
                .iter()
                .filter(|p| p.id != "general")
                .map(|p| p.label.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    caveats.push(flatten(&c.profile_origin_note));
    caveats.push(
        "The APHA Public Health Code of Ethics itself could not be read (apha.org is blocked); its \
         six core values and analysis steps are taken from open-access papers that describe it, and \
         Kass's full article was not read. Check both against the originals."
            .into(),
    );
    caveats.extend(c.copyright_notes.iter().cloned());
    caveats.extend(content::review_caveats(&[ETHICS]));

    Ok(Envelope::new(
        summary,
        EthicsResult {
            decision,
            profile,
            process,
            kass_questions: kass,
            open_questions,
            burden_categories,
            balancing_criteria: balancing,
            least_restrictive_table: table,
            justification_checklist: checklist,
            apha_core_values: values,
            apha_analysis_steps: apha_steps,
            document_file,
            no_recommendation: no_rec,
            copyright_notes: c.copyright_notes.clone(),
            sources_not_read,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        files_written,
        notes: vec![
            "No local files are read. What you type about the decision and situation appears only \
             in the worksheet in /data/output; describe groups, not individual people."
                .into(),
        ],
        ..Privacy::none()
    })
    .decision_support())
}

#[allow(clippy::too_many_arguments)]
fn summary_text(
    decision: &Option<String>,
    profile: &ProfileOut,
    matched: bool,
    decision_type: &Option<String>,
    kass: &[KassQuestionOut],
    table: &[OptionRow],
    checklist: &[ChecklistItem],
    document: &Option<String>,
    no_rec: &str,
) -> String {
    let mut s = String::from("**Public health ethics analysis: Kass framework and APHA Code of Ethics (2019)**\n\n");
    if let Some(d) = decision {
        s.push_str(&format!("Decision: {d}\n\n"));
    }
    s.push_str(&format!(
        "Profile: {}{}.\n\n",
        profile.label,
        if matched || decision_type.is_none() { "" } else { " (no specific profile matched)" }
    ));
    if !profile.tensions.is_empty() {
        s.push_str("Tensions to weigh:\n\n");
        for t in &profile.tensions {
            s.push_str(&format!("- {t}\n"));
        }
        s.push('\n');
    }
    s.push_str("**Six questions** (Kass; paraphrased from an open-access description)\n\n");
    let rows: Vec<Vec<String>> = kass
        .iter()
        .map(|q| {
            vec![
                q.number.to_string(),
                q.question.clone(),
                if q.response.is_some() { "answered".into() } else { "open".into() },
            ]
        })
        .collect();
    s.push_str(&md::table(&["#", "Question", "Status"], &rows));
    s.push_str("\n**Options, least to most restrictive** (drafting aid; rank yours)\n\n");
    let rows: Vec<Vec<String>> = table
        .iter()
        .map(|r| {
            vec![
                r.level.map(|l| l.to_string()).unwrap_or_else(|| "?".into()),
                r.option.clone(),
                r.origin.clone(),
            ]
        })
        .collect();
    s.push_str(&md::table(&["Level", "Option", "Origin"], &rows));
    s.push_str("\n**Justification checklist**\n\n");
    for x in checklist {
        s.push_str(&format!("- {}: {}\n", x.label, x.question));
    }
    s.push_str(&format!("\n_{no_rec}_\n"));
    if let Some(d) = document {
        s.push_str(&format!("\nWorksheet written to `{d}`.\n"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> EthicsContent {
        content::load(ETHICS).unwrap()
    }

    #[test]
    fn content_is_consistent() {
        let c = content();
        assert_eq!(c.kass_questions.len(), 6);
        for (i, q) in c.kass_questions.iter().enumerate() {
            assert_eq!(q.number as usize, i + 1);
            assert!(c.sources.contains_key(&q.source));
        }
        assert_eq!(c.apha_values.len(), 6);
        assert_eq!(c.burden_categories.len(), 3);
        assert_eq!(c.justification_conditions.len() >= 5, true);
        for p in &c.decision_profiles {
            let levels: Vec<u8> = p.options.iter().map(|o| o.level).collect();
            let mut sorted = levels.clone();
            sorted.sort();
            assert_eq!(levels, sorted, "{} options out of order", p.id);
        }
        assert!(c.decision_profiles.iter().any(|p| p.id == "general"));
    }

    #[test]
    fn profiles_match_on_decision_words() {
        let c = content();
        for (text, id, matched) in [
            ("quarantine of exposed contacts", "quarantine_isolation", true),
            ("Isolation orders", "quarantine_isolation", true),
            ("exclude unimmunized students from school", "school_exclusion", true),
            ("allocation of scarce vaccine", "scarce_resource_allocation", true),
            ("rationing antivirals during a shortage", "scarce_resource_allocation", true),
            ("a mandatory flu shot policy", "general", false),
            ("", "general", false),
        ] {
            let (p, m) = match_profile(&c, text);
            assert_eq!((p.id.as_str(), m), (id, matched), "{text}");
        }
    }

    #[test]
    fn question_keys_accept_numbers_and_ids() {
        let c = content();
        assert_eq!(question_index(&c, "1"), Some(0));
        assert_eq!(question_index(&c, "Q4"), Some(3));
        assert_eq!(question_index(&c, "minimize burdens"), Some(3));
        assert_eq!(question_index(&c, "fairness"), Some(4));
        assert_eq!(question_index(&c, "7"), None);
        assert_eq!(question_index(&c, "nonsense"), None);
    }
}
