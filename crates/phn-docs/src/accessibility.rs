//! Accessibility checks against WCAG 2.1 AA, which Section 508 incorporates
//! (36 CFR 1194, E205.4 and 702.10.1).
//!
//! Automated checks catch structural problems; some criteria (meaningful
//! reading order, use of color alone, sensible alt text) always need a human,
//! and are reported as manual checks.

use std::collections::BTreeMap;

use phn_local::docx::{Block as DocxBlock, DocxDocument};
use phn_local::html::HtmlDocument;
use phn_local::pdf::PdfInfo;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Outcome of a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pass,
    Fail,
    Warning,
    NotApplicable,
    Manual,
}

/// One check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Check {
    pub id: String,
    /// WCAG success criterion.
    pub criterion: String,
    pub status: Status,
    pub message: String,
    /// Locations or examples (shortened).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    /// How to fix it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

/// A full report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub format: String,
    pub checks: Vec<Check>,
    pub passed: usize,
    pub failed: usize,
    pub warnings: usize,
    pub manual: usize,
}

impl Report {
    fn new(format: &str, checks: Vec<Check>) -> Self {
        let count = |s: Status| checks.iter().filter(|c| c.status == s).count();
        Self {
            format: format.into(),
            passed: count(Status::Pass),
            failed: count(Status::Fail),
            warnings: count(Status::Warning),
            manual: count(Status::Manual),
            checks,
        }
    }

    /// True when no automated check failed.
    pub fn passes(&self) -> bool {
        self.failed == 0
    }
}

fn check(id: &str, criterion: &str, status: Status, message: impl Into<String>) -> Check {
    Check {
        id: id.into(),
        criterion: criterion.into(),
        status,
        message: message.into(),
        details: Vec::new(),
        fix: None,
    }
}

impl Check {
    fn details(mut self, d: Vec<String>) -> Self {
        self.details = d.into_iter().take(15).collect();
        self
    }
    fn fix(mut self, f: &str) -> Self {
        self.fix = Some(f.into());
        self
    }
}

fn short(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 50 {
        format!("{}…", s.chars().take(50).collect::<String>())
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Color contrast
// ---------------------------------------------------------------------------

/// Parse a CSS or Word color: `#RGB`, `#RRGGBB`, `RRGGBB`, `rgb(r,g,b)`, or a basic name.
pub fn parse_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s
        .trim()
        .trim_end_matches("!important")
        .trim()
        .to_ascii_lowercase();
    let named = match s.as_str() {
        "black" => Some((0, 0, 0)),
        "white" => Some((255, 255, 255)),
        "gray" | "grey" => Some((128, 128, 128)),
        "silver" => Some((192, 192, 192)),
        "lightgray" | "lightgrey" => Some((211, 211, 211)),
        "darkgray" | "darkgrey" => Some((169, 169, 169)),
        "red" => Some((255, 0, 0)),
        "green" => Some((0, 128, 0)),
        "blue" => Some((0, 0, 255)),
        "navy" => Some((0, 0, 128)),
        "yellow" => Some((255, 255, 0)),
        "orange" => Some((255, 165, 0)),
        "purple" => Some((128, 0, 128)),
        "maroon" => Some((128, 0, 0)),
        "teal" => Some((0, 128, 128)),
        "aqua" | "cyan" => Some((0, 255, 255)),
        "lime" => Some((0, 255, 0)),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    if let Some(inner) = s.strip_prefix("rgb(").or_else(|| s.strip_prefix("rgba(")) {
        let parts: Vec<u8> = inner
            .trim_end_matches(')')
            .split(',')
            .take(3)
            .filter_map(|p| {
                p.trim()
                    .parse::<f32>()
                    .ok()
                    .map(|v| v.clamp(0.0, 255.0) as u8)
            })
            .collect();
        return (parts.len() == 3).then(|| (parts[0], parts[1], parts[2]));
    }
    let hex = s.trim_start_matches('#');
    let hex = if hex.len() == 3 {
        hex.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        hex.to_string()
    };
    if hex.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(&hex, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let ch = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)
}

/// WCAG contrast ratio between two colors (1 to 21).
pub fn contrast_ratio(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

const WEAK_LINK_TEXT: [&str; 10] = [
    "click here",
    "here",
    "click",
    "read more",
    "more",
    "link",
    "this link",
    "learn more",
    "this",
    "go",
];

fn weak_links(texts: &[String]) -> Vec<String> {
    texts
        .iter()
        .filter(|t| {
            let n = t.trim().trim_end_matches('.').to_ascii_lowercase();
            WEAK_LINK_TEXT.contains(&n.as_str()) || n.starts_with("http") || n.starts_with("www.")
        })
        .map(|t| short(t))
        .collect()
}

fn heading_order(levels: &[(u8, String)]) -> Vec<String> {
    let mut issues = Vec::new();
    let mut prev = 0u8;
    for (l, t) in levels {
        if prev > 0 && *l > prev + 1 {
            issues.push(format!(
                "Heading {l} \"{}\" follows heading {prev}",
                short(t)
            ));
        }
        prev = *l;
    }
    issues
}

const LANG: &str = "3.1.1 Language of Page";
const TITLE: &str = "2.4.2 Page Titled";
const INFO_REL: &str = "1.3.1 Info and Relationships";
const NON_TEXT: &str = "1.1.1 Non-text Content";
const CONTRAST: &str = "1.4.3 Contrast (Minimum)";
const LINK_PURPOSE: &str = "2.4.4 Link Purpose (In Context)";
const HEADINGS: &str = "2.4.6 Headings and Labels";
const SEQUENCE: &str = "1.3.2 Meaningful Sequence";
const COLOR: &str = "1.4.1 Use of Color";

fn manual_checks() -> Vec<Check> {
    vec![
        check(
            "reading_order",
            SEQUENCE,
            Status::Manual,
            "Read the document in order (or with a screen reader) and confirm the reading order makes sense, especially around text boxes, columns, and floating images.",
        ),
        check(
            "color_alone",
            COLOR,
            Status::Manual,
            "Confirm that color is never the only way information is conveyed (for example, red text meaning 'required').",
        ),
        check(
            "alt_quality",
            NON_TEXT,
            Status::Manual,
            "Confirm that alt text describes the purpose of each image or chart, not just its file name.",
        ),
    ]
}

// ---------------------------------------------------------------------------
// DOCX
// ---------------------------------------------------------------------------

/// Check a DOCX document.
pub fn check_docx(doc: &DocxDocument) -> Report {
    let mut checks = Vec::new();
    checks.push(match &doc.title {
        Some(t) => check(
            "title",
            TITLE,
            Status::Pass,
            format!("Document title is set: \"{}\".", short(t)),
        ),
        None => check(
            "title",
            TITLE,
            Status::Fail,
            "The document has no title in its properties.",
        )
        .fix("In Word: File > Info > Properties > Title."),
    });
    checks.push(match &doc.language {
        Some(l) => check(
            "language",
            LANG,
            Status::Pass,
            format!("Document language is set ({l})."),
        ),
        None => check(
            "language",
            LANG,
            Status::Fail,
            "No default document language is set.",
        )
        .fix("In Word: Review > Language > Set Proofing Language, then 'Set As Default'."),
    });

    let paras: Vec<_> = doc.paragraphs().collect();
    let headings: Vec<(u8, String)> = paras
        .iter()
        .filter_map(|p| p.heading_level.map(|l| (l, p.text.clone())))
        .filter(|(_, t)| !t.trim().is_empty())
        .collect();
    let text_paras = paras.iter().filter(|p| !p.text.trim().is_empty()).count();
    checks.push(if headings.is_empty() && text_paras > 5 {
        check(
            "headings_present",
            HEADINGS,
            Status::Fail,
            format!("No headings in a document with {text_paras} paragraphs."),
        )
        .fix("Apply the built-in Heading 1, Heading 2, … styles to section titles.")
    } else if headings.is_empty() {
        check(
            "headings_present",
            HEADINGS,
            Status::NotApplicable,
            "Short document without headings.",
        )
    } else {
        check(
            "headings_present",
            HEADINGS,
            Status::Pass,
            format!("{} heading(s) use heading styles.", headings.len()),
        )
    });
    let order = heading_order(&headings);
    checks.push(if order.is_empty() {
        check(
            "heading_order",
            INFO_REL,
            if headings.is_empty() {
                Status::NotApplicable
            } else {
                Status::Pass
            },
            "Heading levels do not skip.",
        )
    } else {
        check(
            "heading_order",
            INFO_REL,
            Status::Warning,
            "Heading levels skip (for example Heading 1 followed by Heading 3).",
        )
        .details(order)
        .fix("Use heading levels in order without skipping.")
    });
    let fake: Vec<String> = paras
        .iter()
        .filter(|p| {
            // Title, subtitle, and caption styles are semantic, not fake headings.
            let semantic_style = [p.style_id.as_deref(), p.style_name.as_deref()]
                .into_iter()
                .flatten()
                .any(|s| {
                    matches!(
                        s.to_ascii_lowercase().as_str(),
                        "title" | "subtitle" | "caption"
                    )
                });
            p.heading_level.is_none()
                && !semantic_style
                && !p.is_list
                && p.all_bold
                && !p.text.trim().is_empty()
                && p.text.trim().chars().count() < 80
                && !p.text.trim().ends_with('.')
        })
        .map(|p| short(&p.text))
        .collect();
    checks.push(if fake.is_empty() {
        check(
            "visual_headings",
            INFO_REL,
            Status::Pass,
            "No bold paragraphs that look like unstyled headings.",
        )
    } else {
        check(
            "visual_headings",
            INFO_REL,
            Status::Warning,
            format!(
                "{} short bold paragraph(s) look like headings but are not styled as headings.",
                fake.len()
            ),
        )
        .details(fake)
        .fix("Apply a Heading style instead of bold formatting.")
    });

    let images: Vec<_> = doc.drawings.iter().filter(|d| !d.decorative).collect();
    let missing: Vec<String> = images
        .iter()
        .filter(|d| d.alt_text.is_none())
        .map(|d| d.name.clone().unwrap_or_else(|| "unnamed image".into()))
        .collect();
    checks.push(if images.is_empty() {
        check("alt_text", NON_TEXT, Status::NotApplicable, "No images.")
    } else if missing.is_empty() {
        check("alt_text", NON_TEXT, Status::Pass, format!("All {} image(s) have alt text.", images.len()))
    } else {
        check("alt_text", NON_TEXT, Status::Fail, format!("{} of {} image(s) have no alt text.", missing.len(), images.len()))
            .details(missing)
            .fix("Right-click each image > View Alt Text, and describe its purpose (or mark it decorative).")
    });

    let tables: Vec<_> = doc.tables().filter(|t| t.rows.len() >= 2).collect();
    let no_header: Vec<String> = tables
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.header_row_marked)
        .map(|(i, t)| {
            format!(
                "Table {} (starts \"{}\")",
                i + 1,
                short(&t.rows[0].join(" | "))
            )
        })
        .collect();
    checks.push(if tables.is_empty() {
        check("table_headers", INFO_REL, Status::NotApplicable, "No data tables.")
    } else if no_header.is_empty() {
        check("table_headers", INFO_REL, Status::Pass, format!("All {} table(s) have a header row.", tables.len()))
    } else {
        check("table_headers", INFO_REL, Status::Fail, format!("{} table(s) have no header row marked.", no_header.len()))
            .details(no_header)
            .fix("Select the first row > Table Properties > Row > 'Repeat as header row at the top of each page'.")
    });
    let merged: usize = tables.iter().map(|t| t.merged_cells).sum();
    if merged > 0 {
        checks.push(
            check("merged_cells", INFO_REL, Status::Warning, format!("{merged} merged cell(s) found; merged cells are hard to navigate with a screen reader."))
                .fix("Use simple tables without merged or split cells."),
        );
    }

    let mut low = Vec::new();
    for r in &doc.colored_runs {
        let (Some(fg), Some(bg)) = (
            parse_color(&r.color),
            parse_color(r.background.as_deref().unwrap_or("FFFFFF")),
        ) else {
            continue;
        };
        let ratio = contrast_ratio(fg, bg);
        let large = r
            .size_pt
            .is_some_and(|s| s >= 18.0 || (r.bold && s >= 14.0));
        let need = if large { 3.0 } else { 4.5 };
        if ratio < need {
            low.push(format!(
                "\"{}\" — #{} on #{}: {ratio:.2}:1 (needs {need}:1)",
                short(&r.text),
                r.color,
                r.background.clone().unwrap_or_else(|| "FFFFFF".into())
            ));
        }
    }
    checks.push(if doc.colored_runs.is_empty() {
        check(
            "contrast",
            CONTRAST,
            Status::Pass,
            "No custom text colors (default black text).",
        )
    } else if low.is_empty() {
        check(
            "contrast",
            CONTRAST,
            Status::Pass,
            "Custom text colors meet the contrast minimum.",
        )
    } else {
        check(
            "contrast",
            CONTRAST,
            Status::Fail,
            format!("{} text run(s) have insufficient contrast.", low.len()),
        )
        .details(low)
        .fix("Use darker text or a lighter background: 4.5:1 for normal text, 3:1 for large text.")
    });

    let weak = weak_links(&doc.hyperlink_texts);
    checks.push(if doc.hyperlink_texts.is_empty() {
        check(
            "link_text",
            LINK_PURPOSE,
            Status::NotApplicable,
            "No hyperlinks.",
        )
    } else if weak.is_empty() {
        check(
            "link_text",
            LINK_PURPOSE,
            Status::Pass,
            "Link text is descriptive.",
        )
    } else {
        check(
            "link_text",
            LINK_PURPOSE,
            Status::Warning,
            format!(
                "{} link(s) use vague text or bare web addresses.",
                weak.len()
            ),
        )
        .details(weak)
        .fix("Use link text that says where the link goes, e.g. 'DPH measles guidance'.")
    });

    // Empty paragraphs used for spacing.
    let mut run = 0;
    let mut max_run = 0;
    for b in &doc.blocks {
        if let DocxBlock::Paragraph(p) = b
            && !p.in_table
            && p.text.trim().is_empty()
        {
            run += 1;
            max_run = max_run.max(run);
            continue;
        }
        run = 0;
    }
    if max_run >= 3 {
        checks.push(
            check("blank_lines", SEQUENCE, Status::Warning, format!("Up to {max_run} empty paragraphs in a row are used for spacing; screen readers announce each one as 'blank'."))
                .fix("Use paragraph spacing or page breaks instead of empty lines."),
        );
    }
    let manual_lists: Vec<String> = paras
        .iter()
        .filter(|p| {
            !p.is_list
                && ["• ", "- ", "* ", "o "]
                    .iter()
                    .any(|m| p.text.trim_start().starts_with(m))
        })
        .map(|p| short(&p.text))
        .collect();
    if !manual_lists.is_empty() {
        checks.push(
            check(
                "manual_lists",
                INFO_REL,
                Status::Warning,
                format!(
                    "{} paragraph(s) look like typed bullets rather than real lists.",
                    manual_lists.len()
                ),
            )
            .details(manual_lists)
            .fix("Use the Bullets or Numbering buttons."),
        );
    }
    checks.extend(manual_checks());
    Report::new("docx", checks)
}

// ---------------------------------------------------------------------------
// HTML
// ---------------------------------------------------------------------------

/// Check an HTML document.
pub fn check_html(doc: &HtmlDocument) -> Report {
    let mut checks = Vec::new();
    checks.push(match &doc.title {
        Some(t) => check(
            "title",
            TITLE,
            Status::Pass,
            format!("Page title: \"{}\".", short(t)),
        ),
        None => check("title", TITLE, Status::Fail, "The page has no <title>.")
            .fix("Add a descriptive <title> in <head>."),
    });
    checks.push(match &doc.language {
        Some(l) => check(
            "language",
            LANG,
            Status::Pass,
            format!("Page language is set ({l})."),
        ),
        None => check(
            "language",
            LANG,
            Status::Fail,
            "The <html> element has no lang attribute.",
        )
        .fix("Add lang=\"en\" (or the page's language) to <html>."),
    });
    checks.push(if doc.headings.is_empty() {
        check(
            "headings_present",
            HEADINGS,
            Status::Fail,
            "The page has no headings.",
        )
        .fix("Mark section titles with <h1>–<h6>.")
    } else {
        check(
            "headings_present",
            HEADINGS,
            Status::Pass,
            format!("{} heading(s).", doc.headings.len()),
        )
    });
    let order = heading_order(&doc.headings);
    checks.push(if order.is_empty() {
        check(
            "heading_order",
            INFO_REL,
            Status::Pass,
            "Heading levels do not skip.",
        )
    } else {
        check(
            "heading_order",
            INFO_REL,
            Status::Warning,
            "Heading levels skip.",
        )
        .details(order)
    });
    let missing: Vec<String> = doc
        .images
        .iter()
        .filter(|i| i.alt.is_none())
        .map(|i| short(&i.src))
        .collect();
    checks.push(if doc.images.is_empty() {
        check("alt_text", NON_TEXT, Status::NotApplicable, "No images.")
    } else if missing.is_empty() {
        check(
            "alt_text",
            NON_TEXT,
            Status::Pass,
            format!("All {} image(s) have an alt attribute.", doc.images.len()),
        )
    } else {
        check(
            "alt_text",
            NON_TEXT,
            Status::Fail,
            format!("{} image(s) have no alt attribute.", missing.len()),
        )
        .details(missing)
        .fix("Add alt text, or alt=\"\" for decorative images.")
    });
    let data_tables: Vec<_> = doc
        .tables
        .iter()
        .filter(|t| !t.presentational && t.rows >= 2)
        .collect();
    let no_th = data_tables.iter().filter(|t| !t.has_th).count();
    checks.push(if data_tables.is_empty() {
        check(
            "table_headers",
            INFO_REL,
            Status::NotApplicable,
            "No data tables.",
        )
    } else if no_th == 0 {
        check(
            "table_headers",
            INFO_REL,
            Status::Pass,
            "All data tables use <th> header cells.",
        )
    } else {
        check(
            "table_headers",
            INFO_REL,
            Status::Fail,
            format!("{no_th} data table(s) have no <th> header cells."),
        )
        .fix("Use <th scope=\"col\"> for column headers.")
    });
    let no_caption = data_tables.iter().filter(|t| !t.has_caption).count();
    if no_caption > 0 {
        checks.push(
            check(
                "table_captions",
                INFO_REL,
                Status::Warning,
                format!("{no_caption} data table(s) have no <caption>."),
            )
            .fix("Add a <caption> describing the table."),
        );
    }
    checks.push(if doc.unlabeled_inputs == 0 {
        check(
            "form_labels",
            "3.3.2 Labels or Instructions",
            Status::Pass,
            "Form fields are labeled (or there are none).",
        )
    } else {
        check(
            "form_labels",
            "3.3.2 Labels or Instructions",
            Status::Fail,
            format!("{} form field(s) have no label.", doc.unlabeled_inputs),
        )
        .fix("Associate each field with a <label for=…> or aria-label.")
    });
    let mut low = Vec::new();
    for c in &doc.colored {
        let (Some(fg), Some(bg)) = (
            parse_color(&c.color),
            parse_color(c.background.as_deref().unwrap_or("#ffffff")),
        ) else {
            continue;
        };
        let ratio = contrast_ratio(fg, bg);
        if ratio < 4.5 {
            low.push(format!(
                "\"{}\" — {} on {}: {ratio:.2}:1",
                short(&c.text),
                c.color,
                c.background.clone().unwrap_or_else(|| "#ffffff".into())
            ));
        }
    }
    checks.push(if low.is_empty() {
        check(
            "contrast",
            CONTRAST,
            Status::Pass,
            "Inline text colors meet the contrast minimum (stylesheets are not checked).",
        )
    } else {
        check(
            "contrast",
            CONTRAST,
            Status::Fail,
            format!(
                "{} element(s) with inline colors have insufficient contrast.",
                low.len()
            ),
        )
        .details(low)
    });
    let weak = weak_links(&doc.link_texts);
    let empty_links = doc
        .link_texts
        .iter()
        .filter(|t| t.trim().is_empty())
        .count();
    checks.push(if doc.link_texts.is_empty() {
        check(
            "link_text",
            LINK_PURPOSE,
            Status::NotApplicable,
            "No links.",
        )
    } else if empty_links > 0 {
        check(
            "link_text",
            LINK_PURPOSE,
            Status::Fail,
            format!("{empty_links} link(s) have no text."),
        )
    } else if weak.is_empty() {
        check(
            "link_text",
            LINK_PURPOSE,
            Status::Pass,
            "Link text is descriptive.",
        )
    } else {
        check(
            "link_text",
            LINK_PURPOSE,
            Status::Warning,
            format!("{} link(s) use vague text.", weak.len()),
        )
        .details(weak)
    });
    checks.extend(manual_checks());
    Report::new("html", checks)
}

// ---------------------------------------------------------------------------
// PDF
// ---------------------------------------------------------------------------

/// Check a PDF.
pub fn check_pdf(info: &PdfInfo) -> Report {
    let mut checks = Vec::new();
    checks.push(if info.marked && info.has_struct_tree {
        check("tagged", INFO_REL, Status::Pass, "The PDF is tagged (has a structure tree).")
    } else {
        check("tagged", INFO_REL, Status::Fail, "The PDF is not tagged, so screen readers cannot tell headings, lists, and tables apart.")
            .fix("Export from Word with 'Document structure tags for accessibility' turned on, or add tags in Acrobat.")
    });
    checks.push(match &info.language {
        Some(l) => check(
            "language",
            LANG,
            Status::Pass,
            format!("Document language is set ({l})."),
        ),
        None => check(
            "language",
            LANG,
            Status::Fail,
            "No document language is set.",
        )
        .fix(
            "Set the language in the source document, or in Acrobat: File > Properties > Advanced.",
        ),
    });
    checks.push(match (&info.title, info.display_doc_title) {
        (Some(t), true) => check(
            "title",
            TITLE,
            Status::Pass,
            format!("Title \"{}\" is set and shown in the title bar.", short(t)),
        ),
        (Some(_), false) => check(
            "title",
            TITLE,
            Status::Warning,
            "A title is set but the viewer is told to show the file name instead.",
        )
        .fix("In Acrobat: File > Properties > Initial View > Show: Document Title."),
        (None, _) => check("title", TITLE, Status::Fail, "No document title is set.")
            .fix("Set the title in the source document's properties."),
    });
    checks.push(if info.pages_without_text.is_empty() {
        check(
            "real_text",
            NON_TEXT,
            Status::Pass,
            "Every page has selectable text.",
        )
    } else {
        check(
            "real_text",
            NON_TEXT,
            Status::Fail,
            format!(
                "{} page(s) have no selectable text; they may be scanned images.",
                info.pages_without_text.len()
            ),
        )
        .details(
            info.pages_without_text
                .iter()
                .map(|p| format!("page {p}"))
                .collect(),
        )
        .fix("Use the original electronic document, or run OCR and tag the result.")
    });
    checks.push(if info.figures == 0 {
        check(
            "alt_text",
            NON_TEXT,
            if info.has_struct_tree {
                Status::NotApplicable
            } else {
                Status::Manual
            },
            "No tagged figures found.",
        )
    } else if info.figures_without_alt == 0 {
        check(
            "alt_text",
            NON_TEXT,
            Status::Pass,
            format!("All {} figure(s) have alt text.", info.figures),
        )
    } else {
        check(
            "alt_text",
            NON_TEXT,
            Status::Fail,
            format!(
                "{} of {} figure(s) have no alt text.",
                info.figures_without_alt, info.figures
            ),
        )
    });
    checks.push(if info.unembedded_fonts.is_empty() {
        check(
            "fonts_embedded",
            "PDF/UA 7.21.4",
            Status::Pass,
            "All fonts are embedded.",
        )
    } else {
        check(
            "fonts_embedded",
            "PDF/UA 7.21.4",
            Status::Warning,
            "Some fonts are not embedded, so text may not display or be read correctly everywhere.",
        )
        .details(info.unembedded_fonts.clone())
    });
    if info.encrypted {
        checks.push(check(
            "encryption",
            "4.1.2 Name, Role, Value",
            Status::Warning,
            "The PDF is encrypted; security settings can block screen readers.",
        ));
    }
    checks.extend(manual_checks());
    Report::new("pdf", checks)
}

/// Group check ids by status (for summaries).
pub fn by_status(report: &Report) -> BTreeMap<String, Vec<String>> {
    let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in &report.checks {
        m.entry(format!("{:?}", c.status).to_lowercase())
            .or_default()
            .push(c.id.clone());
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_matches_wcag_examples() {
        let black = (0, 0, 0);
        let white = (255, 255, 255);
        assert!((contrast_ratio(black, white) - 21.0).abs() < 0.01);
        // #767676 on white is the classic 4.54:1 boundary color.
        assert!((contrast_ratio(parse_color("#767676").unwrap(), white) - 4.54).abs() < 0.01);
        assert!(contrast_ratio(parse_color("#AAAAAA").unwrap(), white) < 4.5);
        assert_eq!(parse_color("rgb(255, 0, 0)"), Some((255, 0, 0)));
        assert_eq!(parse_color("#fff"), Some((255, 255, 255)));
    }

    #[test]
    fn exported_docx_passes_its_own_checks() {
        let doc = crate::docx::tests_support::sample();
        let bytes = crate::docx::to_docx(&doc).unwrap();
        let parsed = phn_local::docx::parse_docx(&bytes).unwrap();
        let report = check_docx(&parsed);
        let failures: Vec<_> = report
            .checks
            .iter()
            .filter(|c| c.status == Status::Fail || c.status == Status::Warning)
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn exported_pdf_passes_its_own_checks() {
        let doc = crate::docx::tests_support::sample();
        let bytes = crate::pdf::to_pdf(&doc).unwrap();
        let info = phn_local::pdf::parse_pdf(&bytes).unwrap();
        let report = check_pdf(&info);
        let problems: Vec<_> = report
            .checks
            .iter()
            .filter(|c| c.status == Status::Fail || c.status == Status::Warning)
            .collect();
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn flags_problems_in_a_poor_docx() {
        let body = r#"<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Introduction</w:t></w:r></w:p>
            <w:p><w:r><w:rPr><w:color w:val="BBBBBB"/></w:rPr><w:t>pale text</w:t></w:r></w:p>
            <w:tbl><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
        let bytes = phn_local_docx_fixture(body);
        let parsed = phn_local::docx::parse_docx(&bytes).unwrap();
        let r = check_docx(&parsed);
        let status = |id: &str| r.checks.iter().find(|c| c.id == id).unwrap().status;
        assert_eq!(status("title"), Status::Fail);
        assert_eq!(status("table_headers"), Status::Fail);
        assert_eq!(status("contrast"), Status::Fail);
        assert_eq!(status("visual_headings"), Status::Warning);
        assert!(!r.passes());
    }

    fn phn_local_docx_fixture(body: &str) -> Vec<u8> {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut z = zip::ZipWriter::new(&mut buf);
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("word/document.xml", o).unwrap();
        write!(z, r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#).unwrap();
        z.finish().unwrap();

        buf.into_inner()
    }

    #[test]
    fn checks_html() {
        let d = phn_local::html::parse_html(
            r#"<html><body><h1>A</h1><h3>B</h3><img src="x.png"><a href="/">click here</a></body></html>"#,
        );
        let r = check_html(&d);
        let status = |id: &str| r.checks.iter().find(|c| c.id == id).unwrap().status;
        assert_eq!(status("language"), Status::Fail);
        assert_eq!(status("title"), Status::Fail);
        assert_eq!(status("alt_text"), Status::Fail);
        assert_eq!(status("heading_order"), Status::Warning);
        assert_eq!(status("link_text"), Status::Warning);
    }
}
