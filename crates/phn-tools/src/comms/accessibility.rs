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
        .privacy(Privacy::none()))
}
