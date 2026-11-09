//! Safe-messaging check for suicide-related drafts (shared by MH-06
//! `plan_school_postvention` and the communication tools).
//!
//! Rules come from `content/comms/safe_messaging.yaml` (paraphrased from
//! *After a Suicide: A Toolkit for Schools*). Findings point to the sentence
//! number and the rule; they never repeat words that describe a method.

use std::sync::LazyLock;

use phn_content::ContentMeta;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self, NamedSource, SAFE_MESSAGING};
use super::textstats;

#[derive(Debug, Clone, Deserialize)]
struct Rule {
    id: String,
    severity: String,
    message: String,
    fix: String,
    patterns: Vec<String>,
    requires: Vec<String>,
    #[serde(default, rename = "except")]
    except_patterns: Vec<String>,
    source: String,
    section: String,
    #[allow(dead_code)]
    confidence: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Required {
    id: String,
    message: String,
    fix: String,
    patterns: Vec<String>,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SafeMessagingContent {
    meta: ContentMeta,
    sources: std::collections::BTreeMap<String, NamedSource>,
    rules: Vec<Rule>,
    required: Vec<Required>,
    checklist: Vec<String>,
}

struct Compiled {
    content: SafeMessagingContent,
    rules: Vec<(Vec<regex::Regex>, Vec<regex::Regex>, Vec<regex::Regex>)>,
    required: Vec<Vec<regex::Regex>>,
}

fn compile(p: &str) -> regex::Regex {
    regex::Regex::new(&format!("(?i){p}")).expect("safe-messaging pattern compiles")
}

static RULES: LazyLock<Compiled> = LazyLock::new(|| {
    let content: SafeMessagingContent = content::load(SAFE_MESSAGING)
        .expect("content/comms/safe_messaging.yaml matches its schema");
    let rules = content
        .rules
        .iter()
        .map(|r| {
            (
                r.patterns.iter().map(|p| compile(p)).collect(),
                r.requires
                    .iter()
                    .map(|p| compile(&format!("\\b{p}")))
                    .collect(),
                r.except_patterns.iter().map(|p| compile(p)).collect(),
            )
        })
        .collect();
    let required = content
        .required
        .iter()
        .map(|r| r.patterns.iter().map(|p| compile(p)).collect())
        .collect();
    Compiled {
        content,
        rules,
        required,
    }
});

/// One problem found in a draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SafeMessagingFinding {
    /// Rule id (method_details, death_scene, outdated_language, simple_cause, glamorizing,
    /// sensational, large_announcement, memorial_on_grounds, or a missing element).
    pub rule: String,
    /// "must_fix", "should_fix", or "missing".
    pub severity: String,
    /// 1-based sentences where the rule matched (empty for missing elements).
    pub sentences: Vec<usize>,
    pub message: String,
    pub fix: String,
    /// Toolkit section and page.
    pub source: String,
}

/// Result of a safe-messaging check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SafeMessagingReport {
    pub findings: Vec<SafeMessagingFinding>,
    /// Problems that must be fixed before the draft is used.
    pub must_fix: usize,
    /// Checks a person must make (the tool cannot detect them).
    pub checklist: Vec<String>,
}

/// Check a draft against the safe-messaging rules.
pub fn check(text: &str) -> SafeMessagingReport {
    let c = &*RULES;
    let parsed = textstats::parse(text);
    let sentences: Vec<String> = parsed
        .blocks
        .iter()
        .flat_map(|b| b.sentences.iter().map(|s| s.text.clone()))
        .collect();
    let mut findings = Vec::new();
    for (rule, (pats, reqs, excepts)) in c.content.rules.iter().zip(&c.rules) {
        let hits: Vec<usize> = sentences
            .iter()
            .enumerate()
            .filter(|(_, s)| pats.iter().any(|p| p.is_match(s)))
            .filter(|(_, s)| reqs.is_empty() || reqs.iter().any(|r| r.is_match(s)))
            .filter(|(_, s)| !excepts.iter().any(|e| e.is_match(s)))
            .map(|(i, _)| i + 1)
            .collect();
        if !hits.is_empty() {
            findings.push(SafeMessagingFinding {
                rule: rule.id.clone(),
                severity: rule.severity.clone(),
                sentences: hits,
                message: rule.message.clone(),
                fix: rule.fix.clone(),
                source: source_label(&rule.source, &rule.section),
            });
        }
    }
    for (req, pats) in c.content.required.iter().zip(&c.required) {
        if !pats.iter().any(|p| p.is_match(text)) {
            findings.push(SafeMessagingFinding {
                rule: req.id.clone(),
                severity: "missing".into(),
                sentences: Vec::new(),
                message: req.message.clone(),
                fix: req.fix.clone(),
                source: source_label(&req.source, &req.section),
            });
        }
    }
    let must_fix = findings.iter().filter(|f| f.severity == "must_fix").count();
    SafeMessagingReport {
        findings,
        must_fix,
        checklist: c.content.checklist.clone(),
    }
}

fn source_label(key: &str, section: &str) -> String {
    let title = RULES
        .content
        .sources
        .get(key)
        .map(|s| s.title.clone())
        .unwrap_or_else(|| key.to_string());
    format!("{title}, {section}")
}

/// Citations for the safe-messaging rules.
pub fn sources() -> Vec<phn_core::Source> {
    let c = &RULES.content;
    content::cite(&c.sources, &["toolkit", "ma_988"], &c.meta.prepared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_method_language_and_missing_help_without_echoing() {
        let r = check(
            "Our student committed suicide because of a breakup. He is at peace now. The body was found near the bridge.",
        );
        let ids: Vec<&str> = r.findings.iter().map(|f| f.rule.as_str()).collect();
        for id in [
            "outdated_language",
            "simple_cause",
            "glamorizing",
            "death_scene",
            "method_details",
            "help_resources",
            "hope_and_help_seeking",
        ] {
            assert!(ids.contains(&id), "{id} missing from {ids:?}");
        }
        assert!(r.must_fix >= 2);
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("bridge"), "method words are never echoed");
    }

    #[test]
    fn a_safe_message_passes() {
        let r = check(
            "We are sad to share that a student in our school has died. Counselors are available today. Help is available: talk with a trusted adult, or call or text 988 any time.",
        );
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        assert!(!r.checklist.is_empty());
    }
}
