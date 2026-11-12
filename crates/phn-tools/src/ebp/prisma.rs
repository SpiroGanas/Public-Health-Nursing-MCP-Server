//! PRISMA 2020 flow counts: arithmetic checks, a Mermaid diagram, and a
//! self-contained accessible SVG with a text alternative.
//!
//! The elements follow the "essential elements" of PRISMA 2020 explanation and
//! elaboration item 16a (see `content/ebp/evidence_synthesis.yaml`). The
//! official figure was not readable when this was built, so the layout is this
//! project's, not the PRISMA template.

use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Largest count accepted (keeps the arithmetic far from overflow).
const MAX_COUNT: i64 = 1_000_000_000;

/// Records found in one source.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct SourceCount {
    /// Database, register, or other source (for example "PubMed").
    pub source: String,
    /// Records identified from that source.
    pub records: i64,
}

/// A reason for excluding reports at the full-text stage.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct ExclusionCount {
    /// The main reason (for example "wrong population").
    pub reason: String,
    /// Reports excluded for that reason.
    pub reports: i64,
}

/// Counts for a PRISMA 2020 flow diagram, as the nurse enters them.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct PrismaCounts {
    /// Records identified in total, before anything is removed.
    pub records_identified: i64,
    /// Optional breakdown by database or register; when given it must add up to records_identified.
    #[serde(default)]
    pub identified_by_source: Vec<SourceCount>,
    /// Duplicate records removed before screening.
    pub duplicates_removed: i64,
    /// Records removed before screening for other reasons (for example marked ineligible by
    /// automation tools). Default 0.
    pub removed_other_reasons: Option<i64>,
    /// Records screened (titles or abstracts).
    pub records_screened: i64,
    /// Records excluded after screening.
    pub records_excluded: i64,
    /// Reports sought for retrieval. Default: records screened minus records excluded.
    pub reports_sought: Option<i64>,
    /// Reports that could not be retrieved. Default 0.
    pub reports_not_retrieved: Option<i64>,
    /// Reports assessed for eligibility (full text).
    pub reports_assessed: i64,
    /// Reports excluded at full text, with the main reason for each exclusion.
    #[serde(default)]
    pub reports_excluded: Vec<ExclusionCount>,
    /// Studies included in the review.
    pub studies_included: i64,
    /// Reports of the included studies. Needed when a study has more than one report; otherwise
    /// it must equal the studies included.
    pub reports_of_included_studies: Option<i64>,
}

/// Counts after the arithmetic checks, with every derived number filled in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PrismaFlow {
    pub records_identified: i64,
    pub identified_by_source: Vec<SourceCount>,
    pub duplicates_removed: i64,
    pub removed_other_reasons: i64,
    pub records_screened: i64,
    pub records_excluded: i64,
    pub reports_sought: i64,
    pub reports_not_retrieved: i64,
    pub reports_assessed: i64,
    pub reports_excluded: Vec<ExclusionCount>,
    pub reports_excluded_total: i64,
    pub reports_of_included_studies: i64,
    pub studies_included: i64,
    /// Numbers that were not entered and were filled in.
    pub defaults_used: Vec<String>,
}

fn clean_label(s: &str, max: usize) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

/// Check the counts add up. Every problem is reported at once, as a tool error the nurse can
/// fix.
pub fn validate(c: &PrismaCounts) -> Result<PrismaFlow> {
    let mut problems: Vec<String> = Vec::new();
    let mut defaults: Vec<String> = Vec::new();
    let mut check_range = |name: &str, v: i64| {
        if !(0..=MAX_COUNT).contains(&v) {
            problems.push(format!(
                "{name} is {v}; counts must be whole numbers from 0 to {MAX_COUNT}"
            ));
            false
        } else {
            true
        }
    };
    let other = c.removed_other_reasons.unwrap_or_else(|| {
        defaults.push("removed_other_reasons = 0".into());
        0
    });
    let not_retrieved = c.reports_not_retrieved.unwrap_or_else(|| {
        defaults.push("reports_not_retrieved = 0".into());
        0
    });
    let mut ok = true;
    for (name, v) in [
        ("records_identified", c.records_identified),
        ("duplicates_removed", c.duplicates_removed),
        ("removed_other_reasons", other),
        ("records_screened", c.records_screened),
        ("records_excluded", c.records_excluded),
        ("reports_not_retrieved", not_retrieved),
        ("reports_assessed", c.reports_assessed),
        ("studies_included", c.studies_included),
    ] {
        ok &= check_range(name, v);
    }
    for (name, v) in [
        ("reports_sought", c.reports_sought),
        (
            "reports_of_included_studies",
            c.reports_of_included_studies,
        ),
    ] {
        if let Some(v) = v {
            ok &= check_range(name, v);
        }
    }
    let mut reasons: Vec<ExclusionCount> = Vec::new();
    for r in &c.reports_excluded {
        let reason = clean_label(&r.reason, 120);
        if reason.is_empty() {
            problems.push("every report exclusion needs a reason".into());
            ok = false;
        }
        ok &= check_range(&format!("reports excluded for '{reason}'"), r.reports);
        reasons.push(ExclusionCount {
            reason,
            reports: r.reports,
        });
    }
    let mut by_source: Vec<SourceCount> = Vec::new();
    for s in &c.identified_by_source {
        let source = clean_label(&s.source, 80);
        if source.is_empty() {
            problems.push("every source in identified_by_source needs a name".into());
            ok = false;
        }
        ok &= check_range(&format!("records from '{source}'"), s.records);
        by_source.push(SourceCount {
            source,
            records: s.records,
        });
    }
    if !ok {
        return Err(PhnError::InvalidInput(format!(
            "The PRISMA counts have errors:\n- {}",
            problems.join("\n- ")
        )));
    }

    if !by_source.is_empty() {
        let sum: i64 = by_source.iter().map(|s| s.records).sum();
        if sum != c.records_identified {
            problems.push(format!(
                "records_identified is {} but the sources add up to {sum}",
                c.records_identified
            ));
        }
    }
    let removed = c.duplicates_removed + other;
    if removed > c.records_identified {
        problems.push(format!(
            "{} duplicates + {other} other records removed is more than the {} records identified",
            c.duplicates_removed, c.records_identified
        ));
    } else if c.records_screened != c.records_identified - removed {
        problems.push(format!(
            "records_screened is {} but {} identified - {} duplicates - {other} other removed = {}",
            c.records_screened,
            c.records_identified,
            c.duplicates_removed,
            c.records_identified - removed
        ));
    }
    if c.records_excluded > c.records_screened {
        problems.push(format!(
            "records_excluded ({}) is more than records_screened ({})",
            c.records_excluded, c.records_screened
        ));
    }
    let sought_expected = c.records_screened - c.records_excluded;
    let sought = match c.reports_sought {
        Some(s) => {
            if c.records_excluded <= c.records_screened && s != sought_expected {
                problems.push(format!(
                    "reports_sought is {s} but {} screened - {} excluded = {sought_expected}",
                    c.records_screened, c.records_excluded
                ));
            }
            s
        }
        None => {
            defaults.push(format!(
                "reports_sought = records screened - records excluded = {sought_expected}"
            ));
            sought_expected
        }
    };
    if not_retrieved > sought {
        problems.push(format!(
            "reports_not_retrieved ({not_retrieved}) is more than reports_sought ({sought})"
        ));
    } else if c.reports_assessed != sought - not_retrieved {
        problems.push(format!(
            "reports_assessed is {} but {sought} sought - {not_retrieved} not retrieved = {}",
            c.reports_assessed,
            sought - not_retrieved
        ));
    }
    let excluded_total: i64 = reasons.iter().map(|r| r.reports).sum();
    let mut included_reports = 0;
    if excluded_total > c.reports_assessed {
        problems.push(format!(
            "the exclusion reasons add up to {excluded_total} reports, more than the {} assessed",
            c.reports_assessed
        ));
    } else {
        included_reports = c.reports_assessed - excluded_total;
        match c.reports_of_included_studies {
            Some(r) if r != included_reports => problems.push(format!(
                "reports_of_included_studies is {r} but {} assessed - {excluded_total} excluded = {included_reports}",
                c.reports_assessed
            )),
            Some(_) => {}
            None => defaults.push(format!(
                "reports_of_included_studies = reports assessed - reports excluded = {included_reports}"
            )),
        }
        if c.studies_included > included_reports {
            problems.push(format!(
                "studies_included ({}) is more than the {included_reports} reports of included studies (a study has at least one report)",
                c.studies_included
            ));
        } else if c.reports_of_included_studies.is_none() && c.studies_included != included_reports {
            problems.push(format!(
                "{} studies but {included_reports} reports of included studies: give \
                 reports_of_included_studies if some studies have more than one report; otherwise \
                 check studies_included and the exclusion reasons",
                c.studies_included
            ));
        } else if c.studies_included == 0 && included_reports > 0 {
            problems.push(format!(
                "{included_reports} reports are included but studies_included is 0"
            ));
        }
    }
    if !problems.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "The PRISMA counts do not add up:\n- {}",
            problems.join("\n- ")
        )));
    }
    Ok(PrismaFlow {
        records_identified: c.records_identified,
        identified_by_source: by_source,
        duplicates_removed: c.duplicates_removed,
        removed_other_reasons: other,
        records_screened: c.records_screened,
        records_excluded: c.records_excluded,
        reports_sought: sought,
        reports_not_retrieved: not_retrieved,
        reports_assessed: c.reports_assessed,
        reports_excluded: reasons,
        reports_excluded_total: excluded_total,
        reports_of_included_studies: included_reports,
        studies_included: c.studies_included,
        defaults_used: defaults,
    })
}

fn n(v: i64) -> String {
    let s = v.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// The boxes of the diagram, as lines of text, in reading order:
/// (stage, main-column lines, side-column lines).
struct Row {
    stage: &'static str,
    main: Vec<String>,
    side: Vec<String>,
}

fn rows(f: &PrismaFlow) -> Vec<Row> {
    let mut identified = vec![format!("Records identified (n = {})", n(f.records_identified))];
    for s in &f.identified_by_source {
        identified.push(format!("{} (n = {})", s.source, n(s.records)));
    }
    let mut reasons = vec![format!(
        "Reports excluded (n = {})",
        n(f.reports_excluded_total)
    )];
    for r in &f.reports_excluded {
        reasons.push(format!("{}: n = {}", r.reason, n(r.reports)));
    }
    vec![
        Row {
            stage: "Identification",
            main: identified,
            side: vec![
                format!(
                    "Records removed before screening (n = {})",
                    n(f.duplicates_removed + f.removed_other_reasons)
                ),
                format!("Duplicates (n = {})", n(f.duplicates_removed)),
                format!("Other reasons (n = {})", n(f.removed_other_reasons)),
            ],
        },
        Row {
            stage: "Screening",
            main: vec![format!("Records screened (n = {})", n(f.records_screened))],
            side: vec![format!(
                "Records excluded (n = {})",
                n(f.records_excluded)
            )],
        },
        Row {
            stage: "Screening",
            main: vec![format!(
                "Reports sought for retrieval (n = {})",
                n(f.reports_sought)
            )],
            side: vec![format!(
                "Reports not retrieved (n = {})",
                n(f.reports_not_retrieved)
            )],
        },
        Row {
            stage: "Screening",
            main: vec![format!(
                "Reports assessed for eligibility (n = {})",
                n(f.reports_assessed)
            )],
            side: reasons,
        },
        Row {
            stage: "Included",
            main: vec![
                format!("Studies included in review (n = {})", n(f.studies_included)),
                format!(
                    "Reports of included studies (n = {})",
                    n(f.reports_of_included_studies)
                ),
            ],
            side: Vec::new(),
        },
    ]
}

/// The diagram as sentences, for people who cannot see it (also the SVG's description).
pub fn text_alternative(f: &PrismaFlow) -> Vec<String> {
    let mut v = Vec::new();
    let mut ident = format!("Identification: {} records identified", n(f.records_identified));
    if !f.identified_by_source.is_empty() {
        let parts: Vec<String> = f
            .identified_by_source
            .iter()
            .map(|s| format!("{} {}", s.source, n(s.records)))
            .collect();
        ident.push_str(&format!(" ({})", parts.join("; ")));
    }
    ident.push_str(&format!(
        ". {} duplicate records and {} records for other reasons were removed before screening.",
        n(f.duplicates_removed),
        n(f.removed_other_reasons)
    ));
    v.push(ident);
    v.push(format!(
        "Screening: {} records were screened and {} were excluded.",
        n(f.records_screened),
        n(f.records_excluded)
    ));
    v.push(format!(
        "{} reports were sought for retrieval and {} could not be retrieved, so {} reports were assessed for eligibility.",
        n(f.reports_sought),
        n(f.reports_not_retrieved),
        n(f.reports_assessed)
    ));
    if f.reports_excluded.is_empty() {
        v.push(format!(
            "{} reports were excluded at full text (no reasons entered).",
            n(f.reports_excluded_total)
        ));
    } else {
        let parts: Vec<String> = f
            .reports_excluded
            .iter()
            .map(|r| format!("{} ({})", r.reason, n(r.reports)))
            .collect();
        v.push(format!(
            "{} reports were excluded at full text: {}.",
            n(f.reports_excluded_total),
            parts.join("; ")
        ));
    }
    v.push(format!(
        "Included: {} studies, described in {} reports.",
        n(f.studies_included),
        n(f.reports_of_included_studies)
    ));
    v
}

// ---------------------------------------------------------------- Mermaid

fn mermaid_text(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '"' => Some('\''),
            '<' | '>' | '[' | ']' | '{' | '}' | '|' | '`' | '\\' | '#' | ';' => None,
            '&' => Some('+'),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

/// Mermaid flowchart text (paste into any Mermaid renderer).
pub fn mermaid(f: &PrismaFlow, title: &str) -> String {
    let rows = rows(f);
    let label = |lines: &[String]| {
        lines
            .iter()
            .map(|l| mermaid_text(l))
            .collect::<Vec<_>>()
            .join("<br/>")
    };
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("title: {}\n", mermaid_text(title)));
    out.push_str("---\nflowchart TD\n");
    let ids = ["A", "B", "C", "D", "E"];
    let side_ids = ["AX", "BX", "CX", "DX"];
    let mut last_stage = "";
    for (i, r) in rows.iter().enumerate() {
        if r.stage != last_stage {
            if !last_stage.is_empty() {
                out.push_str("    end\n");
            }
            out.push_str(&format!("    subgraph S{i}[\"{}\"]\n", r.stage));
            last_stage = r.stage;
        }
        out.push_str(&format!("        {}[\"{}\"]\n", ids[i], label(&r.main)));
        if !r.side.is_empty() {
            out.push_str(&format!("        {}[\"{}\"]\n", side_ids[i], label(&r.side)));
        }
    }
    out.push_str("    end\n");
    for i in 0..rows.len() {
        if i + 1 < rows.len() {
            out.push_str(&format!("    {} --> {}\n", ids[i], ids[i + 1]));
        }
        if !rows[i].side.is_empty() {
            out.push_str(&format!("    {} --> {}\n", ids[i], side_ids[i]));
        }
    }
    out
}

// ---------------------------------------------------------------- SVG

fn xml(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == ' ')
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#39;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// Wrap text on spaces to at most `max` characters a line (by characters, never bytes; a word
/// longer than the line is split).
fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let mut w: Vec<char> = word.chars().collect();
        while w.len() > max {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            lines.push(w.drain(..max).collect());
        }
        let word: String = w.into_iter().collect();
        let need = if cur.is_empty() { 0 } else { 1 } + word.chars().count();
        if cur.chars().count() + need > max && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

const FONT: &str = "DejaVu Sans, Arial, Helvetica, sans-serif";
const LINE_H: i32 = 18;
const PAD: i32 = 10;
const COL_W: i32 = 300;
const MAIN_X: i32 = 70;
const SIDE_X: i32 = 440;
const GAP: i32 = 38;
const WRAP_AT: usize = 38;

struct Box {
    lines: Vec<String>,
    h: i32,
}

fn make_box(lines: &[String]) -> Box {
    let mut wrapped: Vec<String> = Vec::new();
    for l in lines {
        wrapped.extend(wrap(l, WRAP_AT));
    }
    let h = wrapped.len() as i32 * LINE_H + 2 * PAD - 4;
    Box { lines: wrapped, h }
}

fn draw_box(out: &mut String, x: i32, y: i32, b: &Box, h: i32, first_bold: bool) {
    out.push_str(&format!(
        "<rect x=\"{x}\" y=\"{y}\" width=\"{COL_W}\" height=\"{h}\" fill=\"#FFFFFF\" stroke=\"#1F3864\" stroke-width=\"1.5\"/>\n"
    ));
    for (i, l) in b.lines.iter().enumerate() {
        let weight = if first_bold && i == 0 { "bold" } else { "normal" };
        out.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" font-family=\"{FONT}\" font-size=\"13\" font-weight=\"{weight}\" fill=\"#111111\">{}</text>\n",
            x + PAD,
            y + PAD + 10 + i as i32 * LINE_H,
            xml(l)
        ));
    }
}

/// A self-contained SVG of the flow diagram. It has a title and a long description (the text
/// alternative) so screen readers announce it, and no external files or scripts.
pub fn svg(f: &PrismaFlow, title: &str) -> String {
    let rows = rows(f);
    let alt = text_alternative(f);
    // Row geometry.
    struct Placed {
        y: i32,
        h: i32,
        main: Box,
        side: Option<Box>,
        stage: &'static str,
    }
    let mut placed: Vec<Placed> = Vec::new();
    let mut y = 20;
    for r in &rows {
        let main = make_box(&r.main);
        let side = (!r.side.is_empty()).then(|| make_box(&r.side));
        let h = main.h.max(side.as_ref().map_or(0, |s| s.h));
        placed.push(Placed {
            y,
            h,
            main,
            side,
            stage: r.stage,
        });
        y += h + GAP;
    }
    let height = y - GAP + 20;
    let width = SIDE_X + COL_W + 20;
    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" width=\"{width}\" height=\"{height}\" role=\"img\" aria-labelledby=\"prisma-title prisma-desc\" lang=\"en\">\n"
    ));
    out.push_str(&format!("<title id=\"prisma-title\">{}</title>\n", xml(title)));
    out.push_str(&format!(
        "<desc id=\"prisma-desc\">{}</desc>\n",
        xml(&alt.join(" "))
    ));
    out.push_str(
        "<defs><marker id=\"arrow\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"7\" markerHeight=\"7\" orient=\"auto-start-reverse\"><path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"#1F3864\"/></marker></defs>\n",
    );
    out.push_str(&format!(
        "<rect x=\"0\" y=\"0\" width=\"{width}\" height=\"{height}\" fill=\"#FFFFFF\"/>\n"
    ));
    // Stage bands.
    let mut i = 0;
    while i < placed.len() {
        let stage = placed[i].stage;
        let mut j = i;
        while j + 1 < placed.len() && placed[j + 1].stage == stage {
            j += 1;
        }
        let top = placed[i].y;
        let bottom = placed[j].y + placed[j].h;
        out.push_str(&format!(
            "<rect x=\"10\" y=\"{top}\" width=\"36\" height=\"{}\" fill=\"#DCE6F2\" stroke=\"#1F3864\"/>\n",
            bottom - top
        ));
        let cy = (top + bottom) / 2;
        out.push_str(&format!(
            "<text transform=\"translate(33 {cy}) rotate(-90)\" text-anchor=\"middle\" font-family=\"{FONT}\" font-size=\"13\" font-weight=\"bold\" fill=\"#1F3864\">{}</text>\n",
            xml(stage)
        ));
        i = j + 1;
    }
    for (k, p) in placed.iter().enumerate() {
        draw_box(&mut out, MAIN_X, p.y, &p.main, p.h, k == 0 || k == placed.len() - 1);
        if let Some(s) = &p.side {
            draw_box(&mut out, SIDE_X, p.y, s, p.h, true);
            let ay = p.y + p.h / 2;
            out.push_str(&format!(
                "<path d=\"M {} {ay} L {} {ay}\" stroke=\"#1F3864\" stroke-width=\"1.5\" fill=\"none\" marker-end=\"url(#arrow)\"/>\n",
                MAIN_X + COL_W,
                SIDE_X - 2
            ));
        }
        if k + 1 < placed.len() {
            let x = MAIN_X + COL_W / 2;
            out.push_str(&format!(
                "<path d=\"M {x} {} L {x} {}\" stroke=\"#1F3864\" stroke-width=\"1.5\" fill=\"none\" marker-end=\"url(#arrow)\"/>\n",
                p.y + p.h,
                placed[k + 1].y - 2
            ));
        }
    }
    out.push_str("</svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worked example: 1,333 records, as in the PRISMA 2020 explanation and elaboration's
    /// example text for item 16a (1,333 records found, 1,092 screened after duplicate removal,
    /// 34 full-text documents reviewed, 6 papers included); the other numbers are made up.
    fn example() -> PrismaCounts {
        PrismaCounts {
            records_identified: 1333,
            identified_by_source: vec![
                SourceCount {
                    source: "PubMed".into(),
                    records: 800,
                },
                SourceCount {
                    source: "Other database".into(),
                    records: 533,
                },
            ],
            duplicates_removed: 241,
            removed_other_reasons: None,
            records_screened: 1092,
            records_excluded: 1050,
            reports_sought: None,
            reports_not_retrieved: Some(2),
            reports_assessed: 40,
            reports_excluded: vec![
                ExclusionCount {
                    reason: "Wrong population".into(),
                    reports: 20,
                },
                ExclusionCount {
                    reason: "No comparison group".into(),
                    reports: 14,
                },
            ],
            studies_included: 6,
            reports_of_included_studies: None,
        }
    }

    #[test]
    fn a_consistent_flow_is_accepted_and_defaults_are_reported() {
        let f = validate(&example()).unwrap();
        assert_eq!(f.reports_sought, 42);
        assert_eq!(f.reports_assessed, 40);
        assert_eq!(f.reports_excluded_total, 34);
        assert_eq!(f.reports_of_included_studies, 6);
        assert_eq!(f.removed_other_reasons, 0);
        assert!(f.defaults_used.iter().any(|d| d.contains("reports_sought")));
        assert!(f.defaults_used.iter().any(|d| d.contains("removed_other_reasons")));
    }

    #[test]
    fn every_arithmetic_error_is_reported() {
        let mut c = example();
        c.records_screened = 1000; // should be 1092
        c.records_excluded = 1100; // more than screened
        c.reports_assessed = 50; // should be sought - not retrieved
        c.reports_excluded[0].reports = 60; // more than assessed
        c.identified_by_source[0].records = 700; // sources no longer add up
        let e = validate(&c).unwrap_err().to_string();
        for want in [
            "sources add up to 1233",
            "records_screened is 1000 but 1333 identified - 241 duplicates - 0 other removed = 1092",
            "records_excluded (1100) is more than records_screened (1000)",
            "reports_assessed is 50",
            "exclusion reasons add up to 74",
        ] {
            assert!(e.contains(want), "missing '{want}' in:\n{e}");
        }
    }

    #[test]
    fn negative_empty_and_huge_counts_are_errors() {
        let mut c = example();
        c.duplicates_removed = -1;
        assert!(validate(&c).unwrap_err().to_string().contains("duplicates_removed is -1"));
        let mut c = example();
        c.reports_excluded[0].reason = "  ".into();
        assert!(validate(&c).unwrap_err().to_string().contains("needs a reason"));
        let mut c = example();
        c.records_identified = i64::MAX;
        assert!(validate(&c).is_err());
    }

    #[test]
    fn included_studies_and_reports() {
        // Two reports for one study needs reports_of_included_studies.
        let mut c = example();
        c.studies_included = 5;
        let e = validate(&c).unwrap_err().to_string();
        assert!(e.contains("give reports_of_included_studies"), "{e}");
        c.reports_of_included_studies = Some(6);
        assert_eq!(validate(&c).unwrap().studies_included, 5);
        // More studies than reports is impossible; a wrong report total is caught.
        c.studies_included = 7;
        assert!(validate(&c).unwrap_err().to_string().contains("more than the 6 reports"));
        c.studies_included = 5;
        c.reports_of_included_studies = Some(9);
        assert!(validate(&c).unwrap_err().to_string().contains("reports_of_included_studies is 9"));
        // Nothing included is a valid, if sad, review.
        let empty = PrismaCounts {
            records_identified: 10,
            duplicates_removed: 2,
            records_screened: 8,
            records_excluded: 8,
            reports_assessed: 0,
            studies_included: 0,
            ..PrismaCounts::default()
        };
        let f = validate(&empty).unwrap();
        assert_eq!((f.reports_sought, f.studies_included), (0, 0));
    }

    #[test]
    fn mermaid_has_every_count_and_escapes_text() {
        let mut c = example();
        c.reports_excluded[0].reason = "Wrong \"population\" <b>x</b> [y] #z; {w}".into();
        let f = validate(&c).unwrap();
        let m = mermaid(&f, "My review");
        assert!(m.starts_with("---\ntitle: My review\n---\nflowchart TD\n"));
        for want in [
            "Records identified (n = 1,333)",
            "Duplicates (n = 241)",
            "Records screened (n = 1,092)",
            "Reports assessed for eligibility (n = 40)",
            "Studies included in review (n = 6)",
        ] {
            assert!(m.contains(want), "{want}");
        }
        assert!(m.contains("Wrong 'population' bx/b y z w: n = 20"));
        // Every node that is declared is closed, and subgraphs are balanced.
        assert_eq!(m.matches("subgraph").count(), m.matches("    end\n").count());
        assert!(!m.contains("<b>"));
    }

    #[test]
    fn svg_is_self_contained_accessible_and_escaped() {
        let mut c = example();
        c.reports_excluded[1].reason = "Used <script>alert(1)</script> & \"quotes\" é ü 研究".into();
        let f = validate(&c).unwrap();
        let s = svg(&f, "PRISMA flow: school asthma programs");
        assert!(s.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(s.contains("role=\"img\"") && s.contains("aria-labelledby=\"prisma-title prisma-desc\""));
        assert!(s.contains("<title id=\"prisma-title\">PRISMA flow: school asthma programs</title>"));
        assert!(s.contains("<desc id=\"prisma-desc\">Identification: 1,333 records identified"));
        assert!(!s.contains("<script>") && !s.contains("http://") || s.matches("http://").count() == 1);
        assert!(s.contains("&lt;script&gt;") && s.contains("研究"));
        assert!(!s.contains("href=") && !s.contains("<image"));
        // It parses as XML and renders.
        let png = phn_docs::svg::svg_to_png(&s, 1.0).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        // Every number appears in the text alternative.
        let alt = text_alternative(&f).join(" ");
        for want in ["1,333", "241", "1,092", "1,050", "42", "40", "34", "6 studies"] {
            assert!(alt.contains(want), "{want} in {alt}");
        }
    }

    #[test]
    fn wrapping_is_by_characters_and_splits_long_words() {
        let w = wrap("ééééééééééééé ü 研究研究研究研究研究研究研究研究研究研究", 8);
        assert!(w.iter().all(|l| l.chars().count() <= 8), "{w:?}");
        assert_eq!(wrap("one two three", 7), vec!["one two", "three"]);
        assert!(wrap("", 8).is_empty());
    }
}
