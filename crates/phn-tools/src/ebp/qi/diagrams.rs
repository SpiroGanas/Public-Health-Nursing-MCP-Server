//! Driver diagram, fishbone (cause-and-effect) diagram, and process map for
//! `plan_quality_improvement`: Mermaid text for pasting into a Mermaid-aware tool, an accessible
//! SVG (with a title, a description, and labels that do not depend on color), and a text
//! alternative (an outline) for every diagram.

use phn_docs::docx::esc;
use phn_docs::fonts::wrap;
use phn_docs::palette::{AXIS, INK, INK_SECONDARY, SURFACE, categorical};

const FS: f32 = 12.0;
const LH: f32 = 15.0;
const PAD: f32 = 8.0;
const MARGIN: f32 = 20.0;

/// One diagram in all its forms.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub mermaid: String,
    pub svg: String,
    /// Plain-text outline: the text alternative.
    pub outline: Vec<String>,
    /// One-paragraph description (the SVG's `<desc>` and the image alt text).
    pub alt_text: String,
}

fn mermaid_label(s: &str) -> String {
    s.replace('"', "#quot;")
        .replace('<', "#lt;")
        .replace('>', "#gt;")
        .replace(['\n', '\r'], " ")
}

fn header(w: f32, h: f32, title: &str, desc: &str) -> String {
    format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}" "#,
            r#"role="img" aria-labelledby="diagram-title diagram-desc" font-family="DejaVu Sans">"#,
            r#"<title id="diagram-title">{t}</title><desc id="diagram-desc">{d}</desc>"#,
            r#"<defs><marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">"#,
            r#"<path d="M0 0 L10 5 L0 10 z" fill="{ink2}"/></marker></defs>"#,
            r#"<rect width="{w:.0}" height="{h:.0}" fill="{bg}"/>"#
        ),
        w = w,
        h = h,
        t = esc(title),
        d = esc(desc),
        ink2 = INK_SECONDARY,
        bg = SURFACE
    )
}

/// Text lines starting at `y_top`, drawn with the given anchor at `x`.
fn lines_svg(x: f32, y_top: f32, lines: &[String], anchor: &str, size: f32, bold: bool) -> String {
    let mut s = String::new();
    for (i, l) in lines.iter().enumerate() {
        s.push_str(&format!(
            r#"<text x="{x:.1}" y="{y:.1}" font-size="{size}" text-anchor="{anchor}" fill="{INK}"{b}>{t}</text>"#,
            y = y_top + (i as f32 + 1.0) * (size * 1.25) - 3.0,
            b = if bold { r#" font-weight="bold""# } else { "" },
            t = esc(l)
        ));
    }
    s
}

fn rect(x: f32, y: f32, w: f32, h: f32, stroke: &str, dashed: bool, rx: f32) -> String {
    format!(
        r#"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="{rx}" fill="{SURFACE}" stroke="{stroke}" stroke-width="2"{d}/>"#,
        d = if dashed {
            r#" stroke-dasharray="6 4""#
        } else {
            ""
        }
    )
}

fn arrow_path(points: &[(f32, f32)]) -> String {
    let mut d = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        d.push_str(&format!("{}{:.1} {:.1} ", if i == 0 { "M" } else { "L" }, x, y));
    }
    format!(
        r#"<path d="{d}" fill="none" stroke="{INK_SECONDARY}" stroke-width="1.5" marker-end="url(#arrow)"/>"#
    )
}

fn box_height(lines: usize) -> f32 {
    lines as f32 * LH + 2.0 * PAD
}

// ------------------------------------------------------------------ driver diagram

/// A secondary driver and the change ideas to test for it.
#[derive(Debug, Clone, PartialEq)]
pub struct SecondaryDriver {
    pub name: String,
    pub change_ideas: Vec<String>,
}

/// A primary driver with its secondary drivers.
#[derive(Debug, Clone, PartialEq)]
pub struct PrimaryDriver {
    pub name: String,
    pub secondary: Vec<SecondaryDriver>,
}

struct ChangeBox {
    y: f32,
    h: f32,
    lines: Vec<String>,
    placeholder: bool,
}
struct SecBox {
    y: f32,
    h: f32,
    lines: Vec<String>,
    changes: Vec<ChangeBox>,
}
struct PrimBox {
    y: f32,
    h: f32,
    lines: Vec<String>,
    secs: Vec<SecBox>,
}

/// A driver diagram: aim, primary drivers, secondary drivers, change ideas. An empty `drivers`
/// list draws a scaffold with placeholders.
pub fn driver_diagram(aim: &str, drivers: &[PrimaryDriver]) -> Diagram {
    let scaffold = drivers.is_empty();
    let drivers: Vec<PrimaryDriver> = if scaffold {
        (1..=3)
            .map(|i| PrimaryDriver {
                name: format!("[Primary driver {i}]"),
                secondary: vec![SecondaryDriver {
                    name: "[Secondary driver]".into(),
                    change_ideas: vec!["[Change idea to test]".into()],
                }],
            })
            .collect()
    } else {
        drivers.to_vec()
    };
    let aim_text = if aim.trim().is_empty() {
        "[Aim statement]".to_string()
    } else {
        aim.trim().to_string()
    };
    let (w_aim, w_p, w_s, w_c, gap) = (200.0f32, 210.0f32, 230.0f32, 250.0f32, 46.0f32);
    let x_aim = MARGIN;
    let x_p = x_aim + w_aim + gap;
    let x_s = x_p + w_p + gap;
    let x_c = x_s + w_s + gap;
    let total_w = x_c + w_c + MARGIN;
    let top = MARGIN + 30.0;

    let wrap_in = |t: &str, w: f32| wrap(t, false, FS, w - 2.0 * PAD);
    let mut y = top;
    let mut prims: Vec<PrimBox> = Vec::new();
    for p in &drivers {
        let p_lines = wrap_in(&p.name, w_p);
        let p_h = box_height(p_lines.len());
        let p_start = y;
        let mut secs: Vec<SecBox> = Vec::new();
        let sec_defs: Vec<SecondaryDriver> = if p.secondary.is_empty() {
            Vec::new()
        } else {
            p.secondary.clone()
        };
        for s in &sec_defs {
            let s_lines = wrap_in(&s.name, w_s);
            let s_h = box_height(s_lines.len());
            let ideas: Vec<(Vec<String>, bool)> = if s.change_ideas.is_empty() {
                vec![(wrap_in("[Add change ideas to test]", w_c), true)]
            } else {
                s.change_ideas
                    .iter()
                    .map(|c| (wrap_in(c, w_c), false))
                    .collect()
            };
            let heights: Vec<f32> = ideas.iter().map(|(l, _)| box_height(l.len())).collect();
            let stack: f32 = heights.iter().sum::<f32>() + 8.0 * (ideas.len() as f32 - 1.0);
            let block = stack.max(s_h);
            let mut cy = y + (block - stack) / 2.0;
            let mut changes = Vec::new();
            for ((lines, placeholder), h) in ideas.into_iter().zip(heights) {
                changes.push(ChangeBox {
                    y: cy,
                    h,
                    lines,
                    placeholder,
                });
                cy += h + 8.0;
            }
            secs.push(SecBox {
                y: y + (block - s_h) / 2.0,
                h: s_h,
                lines: s_lines,
                changes,
            });
            y += block + 10.0;
        }
        let block = (y - p_start - if secs.is_empty() { 0.0 } else { 10.0 }).max(p_h);
        prims.push(PrimBox {
            y: p_start + (block - p_h) / 2.0,
            h: p_h,
            lines: p_lines,
            secs,
        });
        y = p_start + block + 18.0;
    }
    let bottom = y - 18.0 + MARGIN;
    let aim_lines = wrap_in(&aim_text, w_aim);
    let aim_h = box_height(aim_lines.len());
    let aim_y = top + ((bottom - MARGIN - top) - aim_h) / 2.0;

    let mut outline = vec![format!("Aim: {aim_text}")];
    for (i, p) in drivers.iter().enumerate() {
        outline.push(format!("Primary driver {}: {}", i + 1, p.name));
        for (j, s) in p.secondary.iter().enumerate() {
            outline.push(format!("  Secondary driver {}.{}: {}", i + 1, j + 1, s.name));
            for c in &s.change_ideas {
                outline.push(format!("    Change idea: {c}"));
            }
        }
    }
    let alt = format!(
        "Driver diagram{}. Four columns read left to right: the aim, {} primary driver(s), {} secondary driver(s), and {} change idea(s) to test. Aim: {}.",
        if scaffold { " scaffold (blank)" } else { "" },
        drivers.len(),
        drivers.iter().map(|p| p.secondary.len()).sum::<usize>(),
        drivers
            .iter()
            .flat_map(|p| p.secondary.iter())
            .map(|s| s.change_ideas.len())
            .sum::<usize>(),
        aim_text
    );

    // ---- svg
    let mut svg = header(total_w, bottom, "Driver diagram", &alt);
    for (x, label) in [
        (x_aim, "Aim"),
        (x_p, "Primary drivers"),
        (x_s, "Secondary drivers"),
        (x_c, "Change ideas to test"),
    ] {
        svg.push_str(&lines_svg(x, MARGIN - 6.0, &[label.to_string()], "start", FS, true));
    }
    let aim_mid = aim_y + aim_h / 2.0;
    let elbow = |x1: f32, y1: f32, x2: f32, y2: f32| {
        let xm = x1 + (x2 - x1) / 2.0;
        arrow_path(&[(x1, y1), (xm, y1), (xm, y2), (x2, y2)])
    };
    for p in &prims {
        svg.push_str(&elbow(x_aim + w_aim, aim_mid, x_p, p.y + p.h / 2.0));
        for s in &p.secs {
            svg.push_str(&elbow(x_p + w_p, p.y + p.h / 2.0, x_s, s.y + s.h / 2.0));
            for c in &s.changes {
                svg.push_str(&elbow(x_s + w_s, s.y + s.h / 2.0, x_c, c.y + c.h / 2.0));
            }
        }
    }
    svg.push_str(&rect(x_aim, aim_y, w_aim, aim_h, categorical(0), false, 6.0));
    svg.push_str(&lines_svg(x_aim + PAD, aim_y + PAD, &aim_lines, "start", FS, true));
    for p in &prims {
        svg.push_str(&rect(x_p, p.y, w_p, p.h, categorical(1), false, 6.0));
        svg.push_str(&lines_svg(x_p + PAD, p.y + PAD, &p.lines, "start", FS, true));
        for s in &p.secs {
            svg.push_str(&rect(x_s, s.y, w_s, s.h, categorical(2), false, 6.0));
            svg.push_str(&lines_svg(x_s + PAD, s.y + PAD, &s.lines, "start", FS, false));
            for c in &s.changes {
                svg.push_str(&rect(x_c, c.y, w_c, c.h, categorical(6), c.placeholder, 6.0));
                svg.push_str(&lines_svg(x_c + PAD, c.y + PAD, &c.lines, "start", FS, false));
            }
        }
    }
    svg.push_str("</svg>");

    // ---- mermaid
    let mut m = String::from("flowchart LR\n");
    m.push_str(&format!("  A[\"Aim: {}\"]\n", mermaid_label(&aim_text)));
    for (i, p) in drivers.iter().enumerate() {
        let pid = format!("P{}", i + 1);
        m.push_str(&format!("  {pid}[\"{}\"]\n  A --> {pid}\n", mermaid_label(&p.name)));
        for (j, s) in p.secondary.iter().enumerate() {
            let sid = format!("{pid}S{}", j + 1);
            m.push_str(&format!("  {sid}[\"{}\"]\n  {pid} --> {sid}\n", mermaid_label(&s.name)));
            for (k, c) in s.change_ideas.iter().enumerate() {
                let cid = format!("{sid}C{}", k + 1);
                m.push_str(&format!("  {cid}([\"{}\"])\n  {sid} --> {cid}\n", mermaid_label(c)));
            }
        }
    }
    Diagram {
        mermaid: m,
        svg,
        outline,
        alt_text: alt,
    }
}

// ------------------------------------------------------------------ fishbone

/// A category of causes on the fishbone.
#[derive(Debug, Clone, PartialEq)]
pub struct CauseCategory {
    pub name: String,
    pub causes: Vec<String>,
}

/// A fishbone (cause-and-effect) diagram.
pub fn fishbone(effect: &str, categories: &[CauseCategory]) -> Diagram {
    let effect_text = if effect.trim().is_empty() {
        "[Problem or effect]".to_string()
    } else {
        effect.trim().to_string()
    };
    let size = 11.0f32;
    let lh = size * 1.25;
    let text_w = 176.0f32;
    let slot_w = 250.0f32;
    let head_w = 170.0f32;
    // Lines for every cause, and the height each category needs.
    let cause_lines: Vec<Vec<Vec<String>>> = categories
        .iter()
        .map(|c| {
            let causes: Vec<String> = if c.causes.is_empty() {
                vec!["[add causes]".to_string()]
            } else {
                c.causes.clone()
            };
            causes.iter().map(|t| wrap(t, false, size, text_w)).collect()
        })
        .collect();
    let need: f32 = cause_lines
        .iter()
        .map(|cs| {
            cs.iter()
                .map(|l| l.len() as f32 * lh + 8.0)
                .sum::<f32>()
        })
        .fold(0.0, f32::max);
    let arm = need + 36.0;
    let label_h = 26.0f32;
    let slots = categories.len().div_ceil(2).max(1);
    let spine_y = MARGIN + label_h + arm;
    let total_h = 2.0 * spine_y;
    let spine_end = MARGIN + slots as f32 * slot_w + 40.0;
    let total_w = spine_end + head_w + MARGIN;

    let head_lines = wrap(&effect_text, true, FS, head_w - 2.0 * PAD);
    let head_h = box_height(head_lines.len());

    let mut outline = vec![format!("Problem (head of the fish): {effect_text}")];
    for c in categories {
        outline.push(format!("Category: {}", c.name));
        if c.causes.is_empty() {
            outline.push("  (no causes entered yet)".into());
        }
        for x in &c.causes {
            outline.push(format!("  Possible cause: {x}"));
        }
    }
    let alt = format!(
        "Fishbone (cause-and-effect) diagram{}. The problem at the head is: {}. {} category branch(es): {}.",
        if categories.iter().all(|c| c.causes.is_empty()) {
            " scaffold (no causes entered yet)"
        } else {
            ""
        },
        effect_text,
        categories.len(),
        categories
            .iter()
            .map(|c| format!("{} ({} cause(s))", c.name, c.causes.len()))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut svg = header(total_w, total_h, "Fishbone (cause-and-effect) diagram", &alt);
    svg.push_str(&arrow_path(&[(MARGIN, spine_y), (spine_end + 0.0, spine_y)]));
    svg.push_str(&rect(
        spine_end,
        spine_y - head_h / 2.0,
        head_w,
        head_h,
        categorical(1),
        false,
        6.0,
    ));
    svg.push_str(&lines_svg(
        spine_end + PAD,
        spine_y - head_h / 2.0 + PAD,
        &head_lines,
        "start",
        FS,
        true,
    ));
    for (i, c) in categories.iter().enumerate() {
        let top = i % 2 == 0;
        let slot = i / 2;
        let xo = MARGIN + slot as f32 * slot_w + 36.0;
        let xi = xo + 70.0;
        let yo = if top {
            spine_y - arm
        } else {
            spine_y + arm
        };
        svg.push_str(&format!(
            r#"<line x1="{xo:.1}" y1="{yo:.1}" x2="{xi:.1}" y2="{spine_y:.1}" stroke="{INK_SECONDARY}" stroke-width="2"/>"#
        ));
        // Category label at the outer end of the bone.
        let label_w = (phn_docs::fonts::text_width(&c.name, true, FS) + 2.0 * PAD).max(70.0);
        let ly = if top { yo - label_h } else { yo };
        svg.push_str(&rect(xo - label_w / 2.0, ly, label_w, label_h, categorical(0), false, 5.0));
        svg.push_str(&lines_svg(xo, ly + 5.0, &[c.name.clone()], "middle", FS, true));
        // Causes, from the outer end toward the spine.
        let mut offset = 12.0f32;
        for lines in &cause_lines[i] {
            let h = lines.len() as f32 * lh + 8.0;
            let yc = if top { yo + offset + h / 2.0 } else { yo - offset - h / 2.0 };
            let frac = (yc - yo) / (spine_y - yo);
            let xb = xo + (xi - xo) * frac;
            svg.push_str(&format!(
                r#"<line x1="{x1:.1}" y1="{yc:.1}" x2="{xb:.1}" y2="{yc:.1}" stroke="{AXIS}" stroke-width="1.5"/>"#,
                x1 = xb - 14.0
            ));
            svg.push_str(&lines_svg(
                xb - 18.0,
                yc - lines.len() as f32 * lh / 2.0,
                lines,
                "end",
                size,
                false,
            ));
            offset += h;
        }
    }
    svg.push_str("</svg>");

    let mut m = String::from("mindmap\n");
    m.push_str(&format!("  root(({}))\n", mermaid_label(&effect_text).replace(['(', ')'], " ")));
    for c in categories {
        m.push_str(&format!("    {}\n", mermaid_label(&c.name).replace(['(', ')'], " ")));
        for x in &c.causes {
            m.push_str(&format!("      {}\n", mermaid_label(x).replace(['(', ')'], " ")));
        }
    }
    Diagram {
        mermaid: m,
        svg,
        outline,
        alt_text: alt,
    }
}

// ------------------------------------------------------------------ process map

/// What a process map step is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Start,
    Step,
    Decision,
    End,
}

/// One step of a process map.
#[derive(Debug, Clone, PartialEq)]
pub struct MapStep {
    pub name: String,
    pub kind: StepKind,
    pub owner: Option<String>,
    pub pain_point: bool,
    /// Decisions: the step a "no" answer goes to (an index into the steps).
    pub no_goes_to: Option<usize>,
}

/// A process map (flowchart) of the steps as they happen today.
pub fn process_map(steps: &[MapStep]) -> Diagram {
    let scaffold = steps.is_empty();
    let steps: Vec<MapStep> = if scaffold {
        vec![
            MapStep {
                name: "[Start: what triggers the process?]".into(),
                kind: StepKind::Start,
                owner: None,
                pain_point: false,
                no_goes_to: None,
            },
            MapStep {
                name: "[First step]".into(),
                kind: StepKind::Step,
                owner: None,
                pain_point: false,
                no_goes_to: None,
            },
            MapStep {
                name: "[Decision point?]".into(),
                kind: StepKind::Decision,
                owner: None,
                pain_point: false,
                no_goes_to: Some(1),
            },
            MapStep {
                name: "[End: what is the result?]".into(),
                kind: StepKind::End,
                owner: None,
                pain_point: false,
                no_goes_to: None,
            },
        ]
    } else {
        steps.to_vec()
    };
    let w = 260.0f32;
    let cx = MARGIN + 30.0 + w / 2.0;
    let vgap = 36.0f32;
    // Box heights.
    struct Laid {
        y: f32,
        h: f32,
        lines: Vec<String>,
    }
    let mut laid: Vec<Laid> = Vec::new();
    let mut y = MARGIN;
    for s in &steps {
        let mut text = s.name.clone();
        if let Some(o) = &s.owner {
            text.push_str(&format!("\n({o})"));
        }
        let (lines, h) = match s.kind {
            StepKind::Decision => {
                let l = wrap(&text, false, FS, 140.0);
                let h = (l.len() as f32 * LH + 36.0).max(76.0);
                (l, h)
            }
            _ => {
                let l = wrap(&text, false, FS, w - 2.0 * PAD - 8.0);
                let h = box_height(l.len()).max(38.0);
                (l, h)
            }
        };
        laid.push(Laid { y, h, lines });
        y += h + vgap;
    }
    let branches = steps.iter().filter(|s| s.no_goes_to.is_some()).count();
    let total_w = cx + w / 2.0 + 40.0 + 16.0 * branches as f32 + 110.0;
    let any_pain = steps.iter().any(|s| s.pain_point);
    let total_h = y - vgap + MARGIN + if any_pain { 26.0 } else { 0.0 };

    let kind_word = |k: StepKind| match k {
        StepKind::Start => "Start",
        StepKind::Step => "Step",
        StepKind::Decision => "Decision",
        StepKind::End => "End",
    };
    let mut outline = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        let mut line = format!("{} {}: {}", kind_word(s.kind), i + 1, s.name);
        if let Some(o) = &s.owner {
            line.push_str(&format!(" (owner: {o})"));
        }
        if s.kind == StepKind::Decision {
            match s.no_goes_to {
                Some(t) if t < steps.len() => {
                    line.push_str(&format!(". If yes, continue to step {}; if no, go to step {}", i + 2, t + 1))
                }
                _ => line.push_str(". If yes, continue; if no, the path is not drawn"),
            }
        }
        if s.pain_point {
            line.push_str(" [pain point]");
        }
        outline.push(line);
    }
    let pain: Vec<String> = steps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.pain_point)
        .map(|(i, _)| (i + 1).to_string())
        .collect();
    let alt = format!(
        "Process map{}: {} steps from top to bottom{}.{}",
        if scaffold { " scaffold (blank)" } else { "" },
        steps.len(),
        if branches > 0 {
            format!(", {branches} decision(s) with a \"no\" path drawn on the right")
        } else {
            String::new()
        },
        if pain.is_empty() {
            String::new()
        } else {
            format!(" Pain points at step(s) {}.", pain.join(", "))
        }
    );

    let mut svg = header(total_w, total_h, "Process map", &alt);
    // Connectors first.
    for i in 0..steps.len().saturating_sub(1) {
        let a = &laid[i];
        let b = &laid[i + 1];
        svg.push_str(&arrow_path(&[(cx, a.y + a.h), (cx, b.y)]));
        if steps[i].kind == StepKind::Decision {
            svg.push_str(&lines_svg(cx + 6.0, a.y + a.h + 2.0, &["Yes".to_string()], "start", 11.0, true));
        }
    }
    let mut branch_n = 0.0f32;
    for (i, s) in steps.iter().enumerate() {
        if let Some(t) = s.no_goes_to.filter(|t| *t < steps.len() && s.kind == StepKind::Decision) {
            let a = &laid[i];
            let b = &laid[t];
            let x_r = cx + w / 2.0 + 26.0 + 16.0 * branch_n;
            branch_n += 1.0;
            svg.push_str(&arrow_path(&[
                (cx + w / 2.0, a.y + a.h / 2.0),
                (x_r, a.y + a.h / 2.0),
                (x_r, b.y + b.h / 2.0),
                (cx + w / 2.0, b.y + b.h / 2.0),
            ]));
            svg.push_str(&lines_svg(
                cx + w / 2.0 + 4.0,
                a.y + a.h / 2.0 - 18.0,
                &["No".to_string()],
                "start",
                11.0,
                true,
            ));
        }
    }
    for (s, l) in steps.iter().zip(&laid) {
        let stroke = categorical(match s.kind {
            StepKind::Start | StepKind::End => 5,
            StepKind::Step => 0,
            StepKind::Decision => 3,
        });
        match s.kind {
            StepKind::Decision => {
                svg.push_str(&format!(
                    r#"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="{SURFACE}" stroke="{stroke}" stroke-width="2"{d}/>"#,
                    cx,
                    l.y,
                    cx + w / 2.0,
                    l.y + l.h / 2.0,
                    cx,
                    l.y + l.h,
                    cx - w / 2.0,
                    l.y + l.h / 2.0,
                    d = if s.pain_point { r#" stroke-dasharray="6 4""# } else { "" }
                ));
            }
            StepKind::Start | StepKind::End => {
                svg.push_str(&rect(cx - w / 2.0, l.y, w, l.h, stroke, s.pain_point, l.h / 2.0));
            }
            StepKind::Step => svg.push_str(&rect(cx - w / 2.0, l.y, w, l.h, stroke, s.pain_point, 6.0)),
        }
        let text_h = l.lines.len() as f32 * LH;
        svg.push_str(&lines_svg(cx, l.y + (l.h - text_h) / 2.0, &l.lines, "middle", FS, false));
        if s.pain_point {
            let (px, py) = (cx - w / 2.0 + 4.0, l.y + 4.0);
            svg.push_str(&format!(
                r#"<circle cx="{px:.1}" cy="{py:.1}" r="9" fill="{INK}"/><text x="{px:.1}" y="{ty:.1}" font-size="12" font-weight="bold" text-anchor="middle" fill="{SURFACE}">!</text>"#,
                ty = py + 4.5
            ));
        }
    }
    if any_pain {
        svg.push_str(&lines_svg(
            MARGIN,
            total_h - MARGIN - 14.0,
            &["! and a dashed outline mark a pain point (delay, rework, or drop-off)".to_string()],
            "start",
            11.0,
            false,
        ));
    }
    svg.push_str("</svg>");

    let mut m = String::from("flowchart TD\n");
    for (i, s) in steps.iter().enumerate() {
        let t = mermaid_label(&match &s.owner {
            Some(o) => format!("{} ({})", s.name, o),
            None => s.name.clone(),
        });
        let id = format!("N{}", i + 1);
        m.push_str(&match s.kind {
            StepKind::Start | StepKind::End => format!("  {id}([\"{t}\"])\n"),
            StepKind::Step => format!("  {id}[\"{t}\"]\n"),
            StepKind::Decision => format!("  {id}{{\"{t}\"}}\n"),
        });
    }
    for (i, s) in steps.iter().enumerate() {
        if i + 1 < steps.len() {
            if s.kind == StepKind::Decision {
                m.push_str(&format!("  N{} -->|Yes| N{}\n", i + 1, i + 2));
            } else {
                m.push_str(&format!("  N{} --> N{}\n", i + 1, i + 2));
            }
        }
        if let Some(t) = s.no_goes_to.filter(|t| *t < steps.len() && s.kind == StepKind::Decision) {
            m.push_str(&format!("  N{} -->|No| N{}\n", i + 1, t + 1));
        }
    }
    if !pain.is_empty() {
        m.push_str("  classDef pain stroke-dasharray: 5 5,stroke-width:3px\n");
        for p in &pain {
            m.push_str(&format!("  class N{p} pain\n"));
        }
    }
    Diagram {
        mermaid: m,
        svg,
        outline,
        alt_text: alt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phn_docs::svg::svg_to_png;

    fn drivers() -> Vec<PrimaryDriver> {
        vec![
            PrimaryDriver {
                name: "Reliable reminders".into(),
                secondary: vec![
                    SecondaryDriver {
                        name: "Reminder list is accurate".into(),
                        change_ideas: vec![
                            "Clean the registry list monthly".into(),
                            "Check phone numbers at every visit & update".into(),
                        ],
                    },
                    SecondaryDriver {
                        name: "Outreach is timely".into(),
                        change_ideas: vec![],
                    },
                ],
            },
            PrimaryDriver {
                name: "Easy access <to> visits".into(),
                secondary: vec![],
            },
        ]
    }

    #[test]
    fn driver_diagram_has_all_forms_and_renders() {
        let d = driver_diagram("Raise on-time vaccination from 70% to 85% by June 2027", &drivers());
        assert!(d.mermaid.starts_with("flowchart LR"));
        assert!(d.mermaid.contains("P1S1C2") && d.mermaid.contains("#lt;to#gt;"));
        assert!(d.outline.iter().any(|l| l.contains("Change idea: Clean the registry")));
        assert!(d.svg.contains("<title id=\"diagram-title\">Driver diagram</title>"));
        assert!(d.svg.contains("&amp;"));
        assert!(d.alt_text.contains("2 primary driver(s), 2 secondary driver(s), and 2 change idea(s)"));
        let png = svg_to_png(&d.svg, 1.0).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn empty_input_draws_a_scaffold() {
        let d = driver_diagram("", &[]);
        assert!(d.alt_text.contains("scaffold"));
        assert!(d.svg.contains("[Primary driver 1]"));
        let f = fishbone("", &[]);
        assert!(f.svg.contains("[Problem or effect]"));
        let p = process_map(&[]);
        assert!(p.alt_text.contains("scaffold") && p.mermaid.contains("-->|No|"));
        for s in [&d.svg, &f.svg, &p.svg] {
            svg_to_png(s, 1.0).unwrap();
        }
    }

    #[test]
    fn fishbone_and_process_map_render() {
        let cats = vec![
            CauseCategory {
                name: "People".into(),
                causes: vec!["Staff unsure who owns reminders".into(), "New hires not trained".into()],
            },
            CauseCategory {
                name: "Methods".into(),
                causes: vec!["No standing order for recall letters sent in the first week".into()],
            },
            CauseCategory {
                name: "Equipment".into(),
                causes: vec![],
            },
        ];
        let f = fishbone("Many two-year-olds are behind on vaccines", &cats);
        assert!(f.mermaid.starts_with("mindmap"));
        assert_eq!(f.outline.iter().filter(|l| l.starts_with("Category:")).count(), 3);
        svg_to_png(&f.svg, 1.0).unwrap();
        let steps = vec![
            MapStep {
                name: "Child arrives".into(),
                kind: StepKind::Start,
                owner: None,
                pain_point: false,
                no_goes_to: None,
            },
            MapStep {
                name: "Record reviewed?".into(),
                kind: StepKind::Decision,
                owner: Some("Nurse".into()),
                pain_point: true,
                no_goes_to: Some(0),
            },
            MapStep {
                name: "Vaccines given".into(),
                kind: StepKind::End,
                owner: None,
                pain_point: false,
                no_goes_to: None,
            },
        ];
        let p = process_map(&steps);
        assert!(p.mermaid.contains("N2 -->|No| N1") && p.mermaid.contains("class N2 pain"));
        assert!(p.outline[1].contains("if no, go to step 1") && p.outline[1].contains("[pain point]"));
        assert!(p.alt_text.contains("Pain points at step(s) 2"));
        svg_to_png(&p.svg, 1.0).unwrap();
    }
}
