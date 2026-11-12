//! The DOCX project plan for `plan_quality_improvement`.

use phn_docs::model::{Block, ImageBlock, Inline, TableBlock};

use super::types::{
    CheckStatus, QiPlanResult, SquireState,
};

/// An image for the plan: PNG bytes, the output path shown in the file, and alt text.
pub(super) struct PlanImage {
    pub png: Vec<u8>,
    pub path: String,
    pub alt: String,
}

/// Images that can go into the plan.
#[derive(Default)]
pub(super) struct PlanImages {
    pub driver: Option<PlanImage>,
    pub fishbone: Option<PlanImage>,
    pub process_map: Option<PlanImage>,
    pub run_chart: Option<PlanImage>,
}

fn para(s: impl Into<String>) -> Block {
    Block::Paragraph(vec![Inline::plain(s)])
}
fn heading(level: u8, s: impl Into<String>) -> Block {
    Block::Heading {
        level,
        text: s.into(),
    }
}
fn list(items: impl IntoIterator<Item = String>) -> Block {
    Block::List {
        ordered: false,
        items: items.into_iter().map(|s| vec![Inline::plain(s)]).collect(),
    }
}
fn table(caption: &str, headers: &[&str], rows: Vec<Vec<String>>) -> Block {
    Block::Table(TableBlock {
        caption: Some(caption.to_string()),
        headers: headers.iter().map(|h| h.to_string()).collect(),
        rows,
    })
}
fn image(i: &PlanImage) -> Block {
    Block::Image(ImageBlock {
        path: i.path.clone(),
        alt: i.alt.clone(),
        data: Some(i.png.clone()),
    })
}

fn status_word(s: CheckStatus) -> &'static str {
    match s {
        CheckStatus::Met => "Met",
        CheckStatus::Missing => "Missing",
        CheckStatus::Attention => "Check",
        CheckStatus::Judgment => "Team judgment",
    }
}

fn state_word(s: SquireState) -> &'static str {
    match s {
        SquireState::Todo => "To do",
        SquireState::InProgress => "In progress",
        SquireState::Done => "Done",
        SquireState::NotApplicable => "Not applicable",
    }
}

fn dash(s: &Option<String>) -> String {
    s.clone().unwrap_or_default()
}

/// Blocks of the plan.
pub(super) fn blocks(
    r: &QiPlanResult,
    images: &PlanImages,
    review_note: &str,
    source_lines: &[String],
    chart_note: Option<&str>,
) -> Vec<Block> {
    let mut b = Vec::new();
    b.push(para(
        "Draft for your improvement team. Built with the Model for Improvement; edit it with your team. It is decision support, not a requirement or a determination about research review.",
    ));
    b.push(para(review_note));

    b.push(heading(1, "1. What are we trying to accomplish? (aim)"));
    if let Some(a) = &r.aim {
        match &a.statement {
            Some(s) => b.push(para(format!("Aim: {s}"))),
            None => b.push(para("[Write the aim: how much improvement, for whom, by when.]")),
        }
        b.push(table(
            "Aim checks",
            &["Check", "Status", "Note"],
            a.checks
                .iter()
                .map(|c| vec![c.label.clone(), status_word(c.status).into(), c.note.clone()])
                .collect(),
        ));
    }

    b.push(heading(1, "2. How will we know a change is an improvement? (measures)"));
    if let Some(m) = &r.measures {
        if m.measures.is_empty() {
            b.push(para(
                "[List an outcome measure, at least one process measure, and a balancing measure, each with an operational definition.]",
            ));
            b.push(table(
                "Measure types",
                &["Type", "What it tells you", "Example"],
                m.types
                    .iter()
                    .map(|t| vec![t.label.clone(), t.plain.clone(), t.example.clone()])
                    .collect(),
            ));
        } else {
            b.push(table(
                "Measures and operational definitions",
                &[
                    "Measure",
                    "Type",
                    "Numerator",
                    "Denominator",
                    "Data source",
                    "Frequency",
                    "Owner (role)",
                    "Still missing",
                ],
                m.measures
                    .iter()
                    .map(|x| {
                        vec![
                            x.name.clone(),
                            x.kind.clone(),
                            dash(&x.numerator),
                            dash(&x.denominator),
                            dash(&x.data_source),
                            dash(&x.frequency),
                            dash(&x.owner),
                            x.missing.join(", "),
                        ]
                    })
                    .collect(),
            ));
        }
        b.push(list(m.checks.iter().map(|c| {
            format!("{}: {}. {}", c.label, status_word(c.status), c.note)
        })));
    }

    b.push(heading(1, "3. What changes can we make? (theory of change)"));
    if let Some(d) = &r.driver_diagram {
        b.push(heading(2, "Driver diagram"));
        match &images.driver {
            Some(i) => b.push(image(i)),
            None => b.push(para(d.alt_text.clone())),
        }
        b.push(list(d.outline.clone()));
    }
    if let Some(d) = &r.fishbone {
        b.push(heading(2, "Cause-and-effect (fishbone) diagram"));
        match &images.fishbone {
            Some(i) => b.push(image(i)),
            None => b.push(para(d.alt_text.clone())),
        }
        b.push(list(d.outline.clone()));
    }
    if let Some(d) = &r.process_map {
        b.push(heading(2, "Process map"));
        match &images.process_map {
            Some(i) => b.push(image(i)),
            None => b.push(para(d.alt_text.clone())),
        }
        b.push(list(d.outline.clone()));
    }

    if let Some(p) = &r.pdsa {
        b.push(heading(1, "4. Testing changes (PDSA cycles)"));
        b.push(para(p.description.text.clone()));
        if p.cycles.is_empty() {
            b.push(para("[Plan the first small test: one change, a prediction, who, when, where, and what data you will collect.]"));
        }
        for c in &p.cycles {
            b.push(heading(2, format!("Cycle {}: {}", c.number, c.change)));
            b.push(table(
                &format!("PDSA cycle {} plan", c.number),
                &["Item", "Plan"],
                vec![
                    vec!["Objective".into(), dash(&c.objective)],
                    vec!["Prediction (written before the test)".into(), dash(&c.prediction)],
                    vec!["Who".into(), dash(&c.who)],
                    vec!["When".into(), dash(&c.when)],
                    vec!["Where".into(), dash(&c.where_)],
                    vec!["Scale (how small)".into(), dash(&c.scale)],
                    vec!["Data to collect".into(), dash(&c.data_to_collect)],
                    vec!["Builds on".into(), dash(&c.builds_on)],
                    vec!["What happened (Do)".into(), String::new()],
                    vec!["Compared with prediction (Study)".into(), String::new()],
                    vec!["Decision and next cycle (Act)".into(), String::new()],
                ],
            ));
        }
        b.push(table(
            "Checks on the PDSA plan",
            &["Check", "Status", "Note"],
            p.checks
                .iter()
                .map(|c| vec![c.label.clone(), status_word(c.status).into(), c.note.clone()])
                .collect(),
        ));
    }

    if let Some(a) = &r.analysis {
        b.push(heading(1, "5. What the data show"));
        b.push(para(format!("Data: {}.", a.data_from)));
        match &images.run_chart {
            Some(i) => b.push(image(i)),
            None => b.push(para(
                chart_note
                    .map(str::to_string)
                    .unwrap_or_else(|| a.text_alternative.clone()),
            )),
        }
        b.push(para(a.text_alternative.clone()));
        if a.findings.is_empty() {
            b.push(para("No signals of non-random variation."));
        } else {
            b.push(table(
                "Signals",
                &["Rule set", "Rule", "Periods", "What it means"],
                a.findings
                    .iter()
                    .map(|f| {
                        vec![
                            f.rule_set.clone(),
                            f.rule.clone(),
                            f.periods.clone(),
                            f.meaning.clone(),
                        ]
                    })
                    .collect(),
            ));
        }
        b.push(list(a.notes.clone()));
    }

    if let Some(s) = &r.squire {
        b.push(heading(1, "6. Writing it up (SQUIRE 2.0 checklist)"));
        b.push(para(s.license_note.clone()));
        b.push(para(format!("Cite as: {}", s.cite_as)));
        b.push(table(
            "SQUIRE 2.0 items",
            &["#", "Item", "What it asks (paraphrase)", "Draws on", "Status"],
            s.items
                .iter()
                .map(|i| {
                    vec![
                        i.number.to_string(),
                        i.name.clone(),
                        i.purpose.clone(),
                        i.plan_section.clone(),
                        state_word(i.status).into(),
                    ]
                })
                .collect(),
        ));
    }

    b.push(heading(1, "Review and approval"));
    b.push(table(
        "Approval",
        &["Date", "Change or review", "By (name and title)", "Signature"],
        vec![vec![String::new(); 4]],
    ));
    b.push(heading(1, "Sources"));
    b.push(list(source_lines.iter().cloned()));
    b
}
