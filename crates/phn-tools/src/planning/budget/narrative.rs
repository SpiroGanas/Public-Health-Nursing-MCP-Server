//! The budget justification (DOCX) drafted from the line items: template text from
//! `content/planning/budget_reference.yaml` filled with local numbers, and `[NURSE: ...]`
//! placeholders for the facts only the nurse knows. The document holds salaries and amounts, so
//! it is written to /data/output only and never returned.

use std::collections::BTreeMap;

use super::calc::{Block, Category, Computed, Funding, IndirectBase, Line, Rates, Treatment};
use super::{IndirectMethodUsed, fmt_money, fmt_pct};

/// Everything the justification needs.
pub struct NarrativeInput<'a> {
    pub project: &'a str,
    pub organization: Option<&'a str>,
    pub lines: &'a [Line],
    pub rates: &'a Rates,
    pub method: IndirectMethodUsed,
    pub computed: &'a Computed,
    pub template: &'a BTreeMap<String, String>,
    pub required_match_text: Option<String>,
}

fn t<'a>(m: &'a BTreeMap<String, String>, k: &str) -> &'a str {
    m.get(k).map(String::as_str).unwrap_or("")
}

/// Replace `{name}` fields and collapse the template's line folding.
fn fill(template: &str, values: &[(&str, String)]) -> String {
    let mut s = template.split_whitespace().collect::<Vec<_>>().join(" ");
    for (k, v) in values {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

fn years_phrase(first: usize, last: usize, n: usize) -> String {
    if first == last {
        if n == 1 {
            "in the budget year".to_string()
        } else {
            format!("in year {first} only")
        }
    } else {
        format!("each year from year {first} to year {last}")
    }
}

fn first_active_amount(l: &Line, c: &Computed, i: usize) -> f64 {
    c.rows[i]
        .amount
        .get(l.first_year - 1)
        .copied()
        .unwrap_or(0.0)
}

fn table(headers: &[String], rows: &[Vec<String>]) -> String {
    let mut s = format!(
        "| {} |\n|{}|\n",
        headers.join(" | "),
        vec!["---"; headers.len()].join("|")
    );
    for r in rows {
        s.push_str(&format!("| {} |\n", r.join(" | ")));
    }
    s
}

fn summary_table(title: &str, b: &Block, n: usize) -> String {
    let mut headers = vec![title.to_string()];
    headers.extend((1..=n).map(|y| format!("Year {y}")));
    headers.push("Total".into());
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row = |label: &str, v: &[f64]| {
        let mut r = vec![label.to_string()];
        r.extend(v.iter().map(|x| fmt_money(*x)));
        r.push(fmt_money(v.iter().sum()));
        rows.push(r);
    };
    row("Personnel", &b.personnel);
    row("Fringe benefits", &b.fringe);
    row("Travel", &b.travel);
    row("Equipment", &b.equipment);
    row("Supplies", &b.supplies);
    row("Contractual", &b.contractual);
    row("Other direct costs", &b.other);
    row("Total direct costs", &b.direct);
    row("Indirect costs", &b.indirect);
    row("Total", &b.total);
    table(&headers, &rows)
}

/// The justification as Markdown (the first `#` heading becomes the document title).
pub fn markdown(inp: &NarrativeInput) -> String {
    let m = inp.template;
    let n = inp.rates.years;
    let years_text = if n == 1 {
        "the 1-year budget period".to_string()
    } else {
        format!("the {n}-year project period")
    };
    let mut out = format!("# {}: {}\n\n", t(m, "title"), inp.project);
    let org = inp
        .organization
        .map(str::to_string)
        .unwrap_or_else(|| "[NURSE: organization name]".to_string());
    out.push_str(&format!("{org}\n\n"));
    out.push_str(&fill(
        t(m, "intro"),
        &[
            ("project", inp.project.to_string()),
            ("years_text", years_text),
        ],
    ));
    out.push_str("\n\n");

    out.push_str(&format!("## {}\n\n", t(m, "summary_heading")));
    out.push_str(&summary_table("Federal request", &inp.computed.federal, n));
    let has_share = inp.lines.iter().any(|l| l.funding == Funding::CostShare);
    if has_share {
        out.push('\n');
        out.push_str(&summary_table("Cost share", &inp.computed.cost_share, n));
    }
    out.push('\n');

    let federal: Vec<(usize, &Line)> = inp
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.funding == Funding::Federal)
        .collect();
    let by_cat = |c: Category| -> Vec<(usize, &Line)> {
        federal
            .iter()
            .filter(|(_, l)| l.category == c)
            .copied()
            .collect()
    };

    // Personnel and fringe.
    let personnel = by_cat(Category::Personnel);
    if !personnel.is_empty() {
        out.push_str(&format!("## {}\n\n", t(m, "personnel_heading")));
        let esc = if inp.rates.salary_escalation > 0.0 && n > 1 {
            format!(
                " Salaries increase {} each year after year 1.",
                fmt_pct(inp.rates.salary_escalation)
            )
        } else if n > 1 {
            " Salaries are held level across the years. [NURSE: say whether a cost-of-living increase applies.]"
                .to_string()
        } else {
            String::new()
        };
        out.push_str(&fill(
            t(m, "personnel_intro"),
            &[("escalation_sentence", esc)],
        ));
        out.push_str("\n\n");
        for (i, l) in &personnel {
            out.push_str(&format!(
                "- {}\n",
                fill(
                    t(m, "personnel_line"),
                    &[
                        ("item", l.item.clone()),
                        ("effort", fmt_pct(l.quantity)),
                        ("salary", fmt_money(l.unit_cost)),
                        ("y1", fmt_money(first_active_amount(l, inp.computed, *i))),
                    ]
                )
            ));
        }
        out.push('\n');
        out.push_str(&format!("## {}\n\n", t(m, "fringe_heading")));
        let mut rates: Vec<String> = Vec::new();
        for (_, l) in &personnel {
            let r = fmt_pct(l.fringe.unwrap_or(inp.rates.fringe));
            if !rates.contains(&r) {
                rates.push(r);
            }
        }
        let rates_text = if rates.len() == 1 {
            rates[0].clone()
        } else {
            format!("{} (by position)", rates.join(", "))
        };
        out.push_str(&fill(t(m, "fringe_text"), &[("rates_text", rates_text)]));
        out.push_str("\n\n");
    }

    // Other categories.
    let sections = [
        (Category::Travel, "travel_heading", "travel_line", None),
        (
            Category::Equipment,
            "equipment_heading",
            "equipment_line",
            Some("equipment_none"),
        ),
        (
            Category::Supplies,
            "supplies_heading",
            "supplies_line",
            None,
        ),
        (
            Category::Contractual,
            "contractual_heading",
            "contract_line",
            None,
        ),
        (Category::Other, "other_heading", "other_line", None),
    ];
    for (cat, head, line_key, none_key) in sections {
        let items = by_cat(cat);
        if items.is_empty() {
            if let Some(nk) = none_key {
                out.push_str(&format!("## {}\n\n{}\n\n", t(m, head), t(m, nk)));
            }
            continue;
        }
        out.push_str(&format!("## {}\n\n", t(m, head)));
        for (_, l) in items {
            let key = if cat == Category::Contractual && l.treatment == Treatment::Subaward {
                "subaward_line"
            } else {
                line_key
            };
            let excl = match l.exclusion {
                Some(e) => format!(
                    " This cost is not in the base for indirect costs ({}).",
                    e.label()
                ),
                None => String::new(),
            };
            out.push_str(&format!(
                "- {}\n",
                fill(
                    t(m, key),
                    &[
                        ("item", l.item.clone()),
                        ("quantity", fmt_qty(l.quantity)),
                        (
                            "unit",
                            if l.unit.is_empty() {
                                "units".to_string()
                            } else {
                                l.unit.clone()
                            }
                        ),
                        ("unit_cost", fmt_money(l.unit_cost)),
                        (
                            "years_text_line",
                            years_phrase(l.first_year, l.last_year, n)
                        ),
                        ("exclusion_sentence", excl),
                    ]
                )
            ));
        }
        out.push('\n');
    }

    // Indirect.
    out.push_str(&format!("## {}\n\n", t(m, "indirect_heading")));
    let rate = fmt_pct(inp.rates.indirect_rate);
    let text = match inp.method {
        IndirectMethodUsed::DeMinimis => fill(t(m, "indirect_de_minimis"), &[("rate", rate)]),
        IndirectMethodUsed::Negotiated => fill(
            t(m, "indirect_negotiated"),
            &[
                ("rate", rate),
                ("base", base_phrase(inp.rates.indirect_base).to_string()),
            ],
        ),
        IndirectMethodUsed::None => t(m, "indirect_none")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    };
    out.push_str(&text);
    out.push_str("\n\n");

    // Cost share.
    out.push_str(&format!("## {}\n\n", t(m, "cost_share_heading")));
    if has_share {
        let total: f64 = inp.computed.cost_share.total.iter().sum();
        out.push_str(&fill(
            t(m, "cost_share_text"),
            &[("cost_share", fmt_money(total))],
        ));
        out.push_str("\n\n");
        if let Some(req) = &inp.required_match_text {
            out.push_str(req);
            out.push_str("\n\n");
        }
        for (i, l) in inp
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.funding == Funding::CostShare)
        {
            let tot: f64 = inp.computed.rows[i].amount.iter().sum::<f64>()
                + inp.computed.rows[i].fringe.iter().sum::<f64>();
            out.push_str(&format!(
                "- {} ({}): {} over the project.\n",
                l.item,
                l.category.label().to_lowercase(),
                fmt_money(tot)
            ));
        }
        out.push('\n');
    } else {
        out.push_str(&fill(t(m, "cost_share_none"), &[]));
        if let Some(req) = &inp.required_match_text {
            out.push(' ');
            out.push_str(req);
        }
        out.push_str("\n\n");
    }
    out.push_str(&fill(t(m, "closing"), &[]));
    out.push('\n');
    out
}

fn base_phrase(b: IndirectBase) -> &'static str {
    match b {
        IndirectBase::Mtdc => "modified total direct costs",
        IndirectBase::TotalDirect => "total direct costs",
        IndirectBase::SalariesAndWages => "salaries and wages",
        IndirectBase::SalariesAndFringe => "salaries and wages plus fringe benefits",
    }
}

fn fmt_qty(q: f64) -> String {
    if (q - q.round()).abs() < 1e-9 {
        format!("{}", q.round() as i64)
    } else {
        format!("{q}")
    }
}

/// Number of `[NURSE: ...]` placeholders in a text.
pub fn placeholder_count(text: &str) -> usize {
    text.matches("[NURSE:").count()
}
