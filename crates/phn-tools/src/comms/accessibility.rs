//! COMM-05 `check_document_accessibility` (Phase 1).

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::accessibility::{self, Report, Status};
use phn_local::extract::FileKind;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;

/// Arguments for `check_document_accessibility`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AccessibilityArgs {
    /// DOCX, HTML, or PDF file in /data.
    pub path: String,
}

/// Result of `check_document_accessibility`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AccessibilityResult {
    pub file: String,
    pub report: Report,
}

/// Check a document against Section 508 / WCAG 2.1 AA.
pub fn check_document_accessibility(
    ctx: &Context,
    args: AccessibilityArgs,
) -> Result<Envelope<AccessibilityResult>> {
    let sandbox = ctx.sandbox()?;
    let path = sandbox.resolve_existing(&args.path)?;
    let label = sandbox.display_path(&path);
    let report = match FileKind::of(&path) {
        FileKind::Docx => accessibility::check_docx(&phn_local::docx::read_docx(&path)?),
        FileKind::Pdf => accessibility::check_pdf(&phn_local::pdf::read_pdf(&path)?),
        FileKind::Html => {
            let src = phn_local::table::decode_text(&std::fs::read(&path)?);
            accessibility::check_html(&phn_local::html::parse_html(&src))
        }
        _ => {
            return Err(PhnError::Unsupported(
                "accessibility checks support DOCX, HTML, and PDF files".into(),
            ));
        }
    };
    let verdict = if report.failed == 0 {
        "No automated check failed"
    } else {
        "Fix the failed checks before publishing"
    };
    let mut summary = format!(
        "**Accessibility check: {label}** ({})\n\n{verdict}: {} passed, {} failed, {} warning(s), {} manual check(s).\n\n",
        report.format.to_uppercase(),
        report.passed,
        report.failed,
        report.warnings,
        report.manual
    );
    for c in report
        .checks
        .iter()
        .filter(|c| matches!(c.status, Status::Fail | Status::Warning))
    {
        summary.push_str(&format!(
            "- **{}** {} — {}{}\n",
            match c.status {
                Status::Fail => "FAIL",
                _ => "WARN",
            },
            c.criterion,
            c.message,
            c.fix
                .as_ref()
                .map(|f| format!(" _Fix:_ {f}"))
                .unwrap_or_default()
        ));
        for d in c.details.iter().take(5) {
            summary.push_str(&format!("  - {d}\n"));
        }
    }
    summary
        .push_str("\nManual checks: reading order, use of color alone, and quality of alt text.\n");
    // The checker replaces possible identifiers in the excerpts it quotes
    // (phn_docs::accessibility); the privacy block reports what it replaced.
    let (redacted, kinds) = redacted_kinds(report.checks.iter().flat_map(|c| {
        std::iter::once(c.message.as_str()).chain(c.details.iter().map(String::as_str))
    }));
    let mut privacy = Privacy::none();
    privacy.notes.push(
        "The report quotes short excerpts of the document (title, headings, first table rows, \
         link text) to locate problems. Possible identifiers in them are replaced with \
         [REDACTED: kind] before the report is returned."
            .into(),
    );
    if redacted {
        privacy.identifiers_detected = if kinds.is_empty() {
            vec!["possible identifiers".into()]
        } else {
            kinds
        };
        privacy
            .notes
            .push("Identifiers were found in the quoted excerpts and redacted.".into());
        summary.push_str(
            "\nSome excerpts contained possible identifiers; they are shown as [REDACTED].\n",
        );
    }
    Ok(Envelope::new(summary, AccessibilityResult { file: label.clone(), report })
        .source(Source::local_file(label))
        .source(
            Source::computed("Web Content Accessibility Guidelines (WCAG) 2.1", "W3C")
                .with_url("https://www.w3.org/TR/WCAG21/"),
        )
        .source(
            Source::computed("Section 508 Standards (36 CFR 1194)", "U.S. Access Board")
                .with_url("https://www.access-board.gov/ict/"),
        )
        .caveat("Automated checks find structural problems only; a full Section 508 review also needs manual testing with a screen reader.")
        .privacy(privacy))
}

/// Whether any text carries a redaction marker, and the identifier categories
/// the markers name (sorted, without repeats).
fn redacted_kinds<'a>(texts: impl Iterator<Item = &'a str>) -> (bool, Vec<String>) {
    let mut any = false;
    let mut kinds: Vec<String> = Vec::new();
    for t in texts {
        for part in t.split("[REDACTED").skip(1) {
            any = true;
            if let Some((kind, _)) = part.strip_prefix(": ").and_then(|p| p.split_once(']'))
                && !kind.trim().is_empty()
                && !kinds.iter().any(|k| k == kind.trim())
            {
                kinds.push(kind.trim().to_string());
            }
        }
    }
    kinds.sort();
    (any, kinds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_block_names_what_was_redacted() {
        let texts = [
            "Table 1 (starts \"[REDACTED: names] | [REDACTED: phone numbers]\")",
            "Heading 3 \"Plan for [REDACTED: names]\" follows heading 1",
            "cut short [REDACTED]…",
            "Document language is set (en-US).",
        ];
        let (any, kinds) = redacted_kinds(texts.iter().copied());
        assert!(any);
        assert_eq!(kinds, vec!["names", "phone numbers"]);
        let (any, kinds) = redacted_kinds(["No headings."].into_iter());
        assert!(!any && kinds.is_empty());
    }

    #[test]
    fn excerpts_from_a_document_come_back_redacted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        std::fs::write(
            dir.path().join("data/visit.html"),
            "<html><body><h1>Visit</h1><h3>Maria Garcia 413-555-0100</h3></body></html>",
        )
        .unwrap();
        let env = check_document_accessibility(
            &ctx,
            AccessibilityArgs {
                path: "visit.html".into(),
            },
        )
        .unwrap();
        let all = serde_json::to_string(&env).unwrap();
        assert!(
            !all.contains("Garcia") && !all.contains("555-0100"),
            "{all}"
        );
        assert!(!env.privacy.identifiers_detected.is_empty());
    }
}
