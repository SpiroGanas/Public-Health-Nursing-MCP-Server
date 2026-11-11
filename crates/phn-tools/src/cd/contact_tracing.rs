//! CD-06 `plan_contact_tracing`: infectious period, contact elicitation
//! window, monitoring and PEP dates, and an XLSX contact-tracking template
//! (with date formulas) written to the output folder.

use chrono::NaiveDate;
use phn_content::disease_rules::{self, Anchor, DayRef, Disease, Role, Rule, Setting};
use phn_core::{Envelope, PhnError, Privacy, Result};
use rust_xlsxwriter::{Format, Formula, Workbook, XlsxError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::engine::{self, DateFacts, add, find_disease, fmt, infectious_dates, parse_date_arg};
use crate::context::Context;
use crate::md;

/// Arguments for `plan_contact_tracing`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct ContactTracingArgs {
    /// Disease of the index case, e.g. "pertussis", "measles", "mumps", "hepatitis A".
    pub disease: String,
    /// Index case onset of the disease's onset event (rash, cough, symptoms), YYYY-MM-DD.
    #[serde(default)]
    pub onset_date: Option<String>,
    /// Positive specimen collection date (used as the anchor when there is no onset).
    #[serde(default)]
    pub positive_specimen_date: Option<String>,
    /// First full day of appropriate treatment for the index case (shortens the
    /// infectious period for some diseases, e.g. pertussis).
    #[serde(default)]
    pub treatment_start_date: Option<String>,
    /// Date the index case began isolation (exposures after it are unlikely).
    #[serde(default)]
    pub isolation_start_date: Option<String>,
    /// Write the XLSX contact-tracking template to /data/output (default true;
    /// skipped with a note when no data folder is mounted).
    #[serde(default)]
    pub write_template: Option<bool>,
    /// Rows for contacts in the template (default 50, at most 500).
    #[serde(default)]
    pub template_rows: Option<u32>,
}

/// A per-contact date rule (computed from each contact's exposure dates).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContactDateRule {
    /// Column label in the template.
    pub label: String,
    /// Days after the anchor (day 0 = the anchor date).
    pub days: i64,
    /// first_exposure or last_exposure.
    pub anchor: String,
    /// The date if the contact's exposure were on the last day of the elicitation window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example_if_exposed_last_infectious_day: Option<String>,
    pub explanation: String,
    pub citation: String,
}

/// Result of `plan_contact_tracing`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContactTracingResult {
    pub disease: String,
    pub onset_event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infectious_period_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infectious_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infectious_through: Option<String>,
    /// Ask about contacts exposed from this date ...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elicitation_from: Option<String>,
    /// ... through this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elicitation_through: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incubation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_definition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_testing: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitoring: Option<String>,
    /// Date rules applied to each contact (also formulas in the template).
    pub per_contact_dates: Vec<ContactDateRule>,
    /// Template path relative to /data, when written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
}

fn rule_cite(src: &str, section: &str) -> String {
    let c = disease_rules::content();
    let t = c
        .sources
        .get(src)
        .map(|s| s.title.clone())
        .unwrap_or_else(|| src.to_string());
    format!("{t}, {section}")
}

fn per_contact_rules(d: &Disease) -> Vec<ContactDateRule> {
    let mut out = Vec::new();
    if let Some(i) = &d.incubation {
        if let Some(min) = i.min_days() {
            out.push(ContactDateRule {
                label: "Symptoms possible from".into(),
                days: min.floor() as i64,
                anchor: "first_exposure".into(),
                example_if_exposed_last_infectious_day: None,
                explanation: format!("First exposure + minimum incubation ({}).", i.describe()),
                citation: rule_cite(&i.source.source, &i.source.section),
            });
        }
        if let Some(max) = i.max_days() {
            out.push(ContactDateRule {
                label: "Symptoms possible through".into(),
                days: max.ceil() as i64,
                anchor: "last_exposure".into(),
                example_if_exposed_last_infectious_day: None,
                explanation: format!("Last exposure + maximum incubation ({}).", i.describe()),
                citation: rule_cite(&i.source.source, &i.source.section),
            });
        }
    }
    if let Some(m) = &d.contact_monitoring {
        out.push(ContactDateRule {
            label: "Monitoring ends".into(),
            days: i64::from(m.days_after_last_exposure),
            anchor: "last_exposure".into(),
            example_if_exposed_last_infectious_day: None,
            explanation: m.text.clone(),
            citation: rule_cite(&m.source.source, &m.source.section),
        });
    }
    for p in &d.pep {
        let anchor = match p.anchor {
            Anchor::LastExposure => "last_exposure",
            _ => "first_exposure",
        };
        out.push(ContactDateRule {
            label: format!("PEP deadline: {}", short(&p.name)),
            days: i64::from(p.within_hours / 24),
            anchor: anchor.into(),
            example_if_exposed_last_infectious_day: None,
            explanation: format!("{} (within {} hours).", p.text, p.within_hours),
            citation: rule_cite(&p.source, &p.section),
        });
    }
    // Contact exclusion anchored on the contact's own exposures. Each column
    // names who the rule covers (settings and every condition, including the
    // "not" ones), so a rule for one group is not read as applying to all.
    struct Excl<'a> {
        start: DayRef,
        end: DayRef,
        conditions: Vec<(&'static str, bool)>,
        settings: Vec<Setting>,
        rule: &'a Rule,
    }
    let mut excl: Vec<Excl<'_>> = Vec::new();
    for r in d.rules.iter().filter(|r| r.role == Role::Contact) {
        let (Some(start), Some(end)) = (&r.start, &r.end.through) else {
            continue;
        };
        let ok = |a: Anchor| matches!(a, Anchor::FirstExposure | Anchor::LastExposure);
        if !ok(start.anchor) || !ok(end.anchor) {
            continue;
        }
        let conditions = r.when.conditions();
        if let Some(e) = excl
            .iter_mut()
            .find(|e| e.start.day == start.day && e.end.day == end.day)
        {
            // The same rule written once per setting: widen the settings.
            if e.start == *start && e.end == *end && e.conditions == conditions {
                for s in &r.settings {
                    if !e.settings.contains(s) {
                        e.settings.push(*s);
                    }
                }
            }
            continue;
        }
        if excl.len() >= 2 {
            continue;
        }
        excl.push(Excl {
            start: *start,
            end: *end,
            conditions,
            settings: r.settings.clone(),
            rule: r,
        });
    }
    for e in &excl {
        let mut parts: Vec<String> = Vec::new();
        let every_setting = e.settings.contains(&Setting::All)
            || Setting::CONCRETE.iter().all(|s| e.settings.contains(s));
        if !every_setting {
            parts.push(
                e.settings
                    .iter()
                    .map(|s| match s {
                        Setting::All => "all settings",
                        Setting::ChildCare => "child care",
                        Setting::School => "school",
                        Setting::FoodHandler => "food handlers",
                        Setting::HealthCareWorker => "health care workers",
                        Setting::Other => "other settings",
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if !e.conditions.is_empty() {
            parts.push(
                e.conditions
                    .iter()
                    .map(|(n, v)| engine::condition_label(n, *v))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        let who = if parts.is_empty() {
            String::new()
        } else {
            format!(" ({})", parts.join("; "))
        };
        let name = |a: Anchor| match a {
            Anchor::FirstExposure => "first_exposure",
            _ => "last_exposure",
        };
        let r = e.rule;
        out.push(ContactDateRule {
            label: format!("Exclude from{who}"),
            days: i64::from(e.start.day),
            anchor: name(e.start.anchor).into(),
            example_if_exposed_last_infectious_day: None,
            explanation: format!("{} — {}", r.applies_to, r.wording),
            citation: r.citation.clone(),
        });
        out.push(ContactDateRule {
            label: format!("Exclude through{who}"),
            days: i64::from(e.end.day),
            anchor: name(e.end.anchor).into(),
            example_if_exposed_last_infectious_day: None,
            explanation: format!("{} — {}", r.applies_to, r.wording),
            citation: r.citation.clone(),
        });
    }
    out
}

fn short(s: &str) -> String {
    let s = s.split(['(', ',']).next().unwrap_or(s).trim();
    s.chars().take(40).collect()
}

fn xerr(e: XlsxError) -> PhnError {
    PhnError::Internal(format!("XLSX: {e}"))
}

fn col_letter(c: u16) -> String {
    let mut n = u32::from(c) + 1;
    let mut s = String::new();
    while n > 0 {
        let r = ((n - 1) % 26) as u8;
        s.insert(0, (b'A' + r) as char);
        n = (n - 1) / 26;
    }
    s
}

struct TemplateInfo<'a> {
    disease: &'a Disease,
    rules: &'a [ContactDateRule],
    key_dates: Vec<(String, String)>,
    rows: u32,
    review: String,
}

fn build_template(t: &TemplateInfo<'_>) -> std::result::Result<Vec<u8>, XlsxError> {
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let head = Format::new().set_bold().set_text_wrap();
    let date = Format::new().set_num_format("yyyy-mm-dd");
    let wrap = Format::new().set_text_wrap();

    // Sheet 1: contacts.
    let ws = wb.add_worksheet();
    ws.set_name("Contacts")?;
    ws.set_header(format!(
        "&LContact tracking: {}&RConfidential: keep in /data/output",
        t.disease.name.replace('&', "&&")
    ));
    ws.set_footer("&LPage &P of &N&R&D");
    let mut headers: Vec<String> = vec![
        "Contact ID".into(),
        "Name".into(),
        "Phone".into(),
        "Relationship or setting".into(),
        "First exposure date".into(),
        "Last exposure date".into(),
    ];
    let first_col: u16 = 4;
    let last_col: u16 = 5;
    let formula_start = headers.len() as u16;
    headers.extend(t.rules.iter().map(|r| r.label.clone()));
    headers.extend(
        [
            "Evidence of immunity / risk notes",
            "PEP given (date and product)",
            "Symptom check dates",
            "Symptoms (date of onset)",
            "Test date",
            "Test result",
            "Outcome / notes",
        ]
        .map(String::from),
    );
    for (c, h) in headers.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, h, &head)?;
        ws.set_column_width(c as u16, if c < 4 { 18 } else { 16 })?;
    }
    ws.set_column_format(first_col, &date)?;
    ws.set_column_format(last_col, &date)?;
    for r in 1..=t.rows {
        for (i, rule) in t.rules.iter().enumerate() {
            let col = formula_start + i as u16;
            let anchor_col = if rule.anchor == "first_exposure" {
                first_col
            } else {
                last_col
            };
            let a = format!("{}{}", col_letter(anchor_col), r + 1);
            let f = format!("=IF({a}=\"\",\"\",{a}+{})", rule.days);
            ws.write_formula_with_format(r, col, Formula::new(f), &date)?;
        }
    }
    ws.set_freeze_panes(1, 2)?;

    // Sheet 2: key dates for the index case and the rules used.
    let ws = wb.add_worksheet();
    ws.set_name("Key dates and rules")?;
    ws.write_string_with_format(0, 0, "Item", &bold)?;
    ws.write_string_with_format(0, 1, "Value", &bold)?;
    ws.set_column_width(0, 34)?;
    ws.set_column_width(1, 90)?;
    let mut row = 1;
    for (k, v) in &t.key_dates {
        ws.write_string(row, 0, k)?;
        ws.write_string_with_format(row, 1, v, &wrap)?;
        row += 1;
    }
    row += 1;
    ws.write_string_with_format(row, 0, "Column", &bold)?;
    ws.write_string_with_format(row, 1, "How it is computed (and source)", &bold)?;
    row += 1;
    for r in t.rules {
        ws.write_string(row, 0, &r.label)?;
        ws.write_string_with_format(
            row,
            1,
            format!(
                "{} + {} days. {} Source: {}",
                r.anchor.replace('_', " "),
                r.days,
                r.explanation,
                r.citation
            ),
            &wrap,
        )?;
        row += 1;
    }
    row += 1;
    ws.write_string_with_format(row, 0, "Review status", &bold)?;
    ws.write_string_with_format(row, 1, &t.review, &wrap)?;
    row += 1;
    ws.write_string_with_format(row, 0, "Privacy", &bold)?;
    ws.write_string_with_format(
        row,
        1,
        "This workbook will hold identifiable information once filled in. Keep it in the \
         output folder or your agency's secure storage; do not paste its contents into the chat.",
        &wrap,
    )?;
    wb.save_to_buffer()
}

/// Plan contact tracing for an index case.
pub fn plan_contact_tracing(
    ctx: &Context,
    args: ContactTracingArgs,
) -> Result<Envelope<ContactTracingResult>> {
    let content = disease_rules::content();
    let disease = find_disease(&args.disease)?;
    let onset = parse_date_arg("onset_date", &args.onset_date)?;
    let specimen = parse_date_arg("positive_specimen_date", &args.positive_specimen_date)?;
    let treatment = parse_date_arg("treatment_start_date", &args.treatment_start_date)?;
    let isolation = parse_date_arg("isolation_start_date", &args.isolation_start_date)?;
    let rows = args.template_rows.unwrap_or(50);
    if !(1..=500).contains(&rows) {
        return Err(PhnError::InvalidInput(
            "template_rows must be between 1 and 500".into(),
        ));
    }
    let mut caveats = vec![engine::pending_caveat(content)];
    let mut used_specimen = false;
    let dates = DateFacts {
        onset: onset.or_else(|| specimen.inspect(|_| used_specimen = true)),
        positive_specimen: specimen,
        treatment_start: treatment,
        ..DateFacts::default()
    };
    if used_specimen {
        caveats.push(format!(
            "No {} date was given, so the positive specimen date was used in its place; the \
             infectious period may start earlier.",
            disease.onset_event
        ));
    }
    let inf = infectious_dates(disease, &dates);
    let (elicit_from, elicit_through): (Option<NaiveDate>, Option<NaiveDate>) = match &inf {
        Some((s, e, _)) => {
            let through = match isolation {
                Some(i) if i <= *e => {
                    if i < *s {
                        return Err(PhnError::InvalidInput(
                            "isolation_start_date is before the infectious period starts".into(),
                        ));
                    }
                    i
                }
                _ => *e,
            };
            (Some(*s), Some(through))
        }
        None => (None, None),
    };
    if inf.is_none() {
        caveats.push(if disease.infectious_period.as_ref().is_some_and(|i| i.anchor.is_some()) {
            format!(
                "Give the {} date (onset_date) to compute the infectious period and elicitation \
                 window.",
                disease.onset_event
            )
        } else {
            "The infectious period for this disease is not a fixed number of days (see the text); \
             the elicitation window is not computed. Set it with DPH."
                .to_string()
        });
    }
    let mut rules = per_contact_rules(disease);
    if let Some(last) = elicit_through {
        for r in &mut rules {
            r.example_if_exposed_last_infectious_day = Some(fmt(add(last, r.days)));
        }
    }

    // Key dates for the template and summary.
    let mut key_dates: Vec<(String, String)> = vec![("Disease".into(), disease.name.clone())];
    if let Some(o) = onset {
        key_dates.push((format!("Index case {}", disease.onset_event), fmt(o)));
    }
    if let Some(sp) = specimen {
        key_dates.push(("Positive specimen".into(), fmt(sp)));
    }
    if let Some(t) = treatment {
        key_dates.push(("Treatment started".into(), fmt(t)));
    }
    if let Some((s, e, how)) = &inf {
        key_dates.push((
            "Infectious period".into(),
            format!("{} through {} ({how})", fmt(*s), fmt(*e)),
        ));
    }
    if let (Some(a), Some(b)) = (elicit_from, elicit_through) {
        key_dates.push((
            "Elicit contacts exposed".into(),
            format!(
                "{} through {}{}",
                fmt(a),
                fmt(b),
                if isolation.is_some_and(|i| Some(i) == elicit_through) {
                    " (isolation began)"
                } else {
                    ""
                }
            ),
        ));
    }
    if let Some(i) = &disease.incubation {
        key_dates.push(("Incubation period".into(), i.describe()));
    }
    if let Some(cd) = &disease.contact_definition {
        key_dates.push(("Who is a contact".into(), cd.clone()));
    }
    if let Some(ct) = &disease.contact_testing {
        key_dates.push(("Testing contacts".into(), ct.clone()));
    }

    // Template.
    let want = args.write_template.unwrap_or(true);
    let mut template = None;
    let mut files_written = Vec::new();
    if want {
        match ctx.sandbox() {
            Ok(sb) => {
                let bytes = build_template(&TemplateInfo {
                    disease,
                    rules: &rules,
                    key_dates: key_dates.clone(),
                    rows,
                    review: engine::pending_caveat(content),
                })
                .map_err(xerr)?;
                let path = sb.new_output_file(&format!("contact_tracking_{}.xlsx", disease.id))?;
                std::fs::write(&path, bytes)?;
                let shown = sb.display_path(&path);
                files_written.push(shown.clone());
                template = Some(shown);
            }
            Err(e) if args.write_template.is_none() => {
                caveats.push(format!(
                    "The contact-tracking spreadsheet was not written because no data folder is \
                     mounted ({e})."
                ));
            }
            Err(e) => return Err(e),
        }
    }

    // ---- Markdown
    let mut s = format!("**Contact tracing plan: {}**\n\n", disease.name);
    let kd_rows: Vec<Vec<String>> = key_dates
        .iter()
        .map(|(k, v)| vec![k.clone(), v.clone()])
        .collect();
    s.push_str(&md::table(&["", ""], &kd_rows));
    if !rules.is_empty() {
        s.push_str("\n**Dates for each contact** (computed in the template from each contact's exposure dates)\n\n");
        let r: Vec<Vec<String>> = rules
            .iter()
            .map(|r| {
                vec![
                    r.label.clone(),
                    format!("{} + {} days", r.anchor.replace('_', " "), r.days),
                    r.example_if_exposed_last_infectious_day
                        .clone()
                        .unwrap_or_else(|| "—".into()),
                    r.citation.clone(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "Date",
                "Rule",
                "If exposed on the last day of the window",
                "Source",
            ],
            &r,
        ));
    }
    if let Some(m) = &disease.contact_monitoring {
        s.push_str(&format!("\n**Monitoring:** {}\n", m.text));
    }
    if let Some(t) = &template {
        s.push_str(&format!(
            "\n**Template:** {t} — one row per contact; enter first and last exposure dates and \
             the date columns fill in. Names and phone numbers stay in that file.\n"
        ));
    }
    s.push_str(
        "\n**Steps:** interview the case about every setting attended during the elicitation \
         window; list contacts in the template (in your files, not in this chat); assess each \
         contact's immunity or risk; offer PEP before its deadline; give exclusion and monitoring \
         dates; report through MAVEN as DPH directs.\n",
    );

    let mut source_ids: Vec<String> = Vec::new();
    if let Some(i) = &disease.incubation {
        source_ids.push(i.source.source.clone());
    }
    if let Some(i) = &disease.infectious_period {
        source_ids.push(i.source.source.clone());
    }
    if let Some(m) = &disease.contact_monitoring {
        source_ids.push(m.source.source.clone());
    }
    source_ids.extend(disease.pep.iter().map(|p| p.source.clone()));
    source_ids.extend(
        disease
            .rules
            .iter()
            .filter(|r| r.role == Role::Contact)
            .map(|r| r.source.clone()),
    );
    source_ids.sort();
    source_ids.dedup();
    let sources: Vec<_> = source_ids
        .iter()
        .map(|id| engine::rule_source(content, id))
        .collect();
    caveats.push(
        "Contact definitions and the elicitation window are starting points; DPH may widen them \
         (for example, an entire school for measles)."
            .into(),
    );
    let mut privacy = Privacy::none();
    privacy.files_written = files_written;
    privacy.notes.push(
        "The template is blank: fill in contacts locally. Do not paste contact names or phone \
         numbers into the chat."
            .into(),
    );
    let data = ContactTracingResult {
        disease: disease.name.clone(),
        onset_event: disease.onset_event.clone(),
        infectious_period_text: disease.infectious_period.as_ref().map(|i| i.text.clone()),
        infectious_from: inf.as_ref().map(|i| fmt(i.0)),
        infectious_through: inf.as_ref().map(|i| fmt(i.1)),
        elicitation_from: elicit_from.map(fmt),
        elicitation_through: elicit_through.map(fmt),
        incubation: disease.incubation.as_ref().map(|i| i.describe()),
        contact_definition: disease.contact_definition.clone(),
        contact_testing: disease.contact_testing.clone(),
        monitoring: disease.contact_monitoring.as_ref().map(|m| m.text.clone()),
        per_contact_dates: rules,
        template,
    };
    Ok(Envelope::new(s, data)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy)
        .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pertussis_plan_writes_template_with_formulas() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = plan_contact_tracing(
            &c,
            ContactTracingArgs {
                disease: "pertussis".into(),
                onset_date: Some("2026-02-01".into()),
                treatment_start_date: Some("2026-02-10".into()),
                ..ContactTracingArgs::default()
            },
        )
        .unwrap();
        let d = &env.data;
        assert_eq!(d.infectious_from.as_deref(), Some("2026-01-25"));
        // Day 5 of treatment (day 1 = Feb 10) is Feb 14.
        assert_eq!(d.infectious_through.as_deref(), Some("2026-02-14"));
        assert_eq!(d.elicitation_through.as_deref(), Some("2026-02-14"));
        let m = d
            .per_contact_dates
            .iter()
            .find(|r| r.label == "Monitoring ends")
            .unwrap();
        assert_eq!(
            m.example_if_exposed_last_infectious_day.as_deref(),
            Some("2026-03-07")
        );
        let t = d.template.as_ref().unwrap();
        let tables = phn_local::table::read_tables(&dir.path().join("data").join(t)).unwrap();
        assert!(tables[0].headers.iter().any(|h| h == "Monitoring ends"));
        assert!(
            tables[0]
                .headers
                .iter()
                .any(|h| h.starts_with("PEP deadline"))
        );
        assert_eq!(env.privacy.files_written.len(), 1);
    }

    #[test]
    fn exclusion_columns_say_who_the_rule_covers() {
        let labels = |q: &str| -> Vec<String> {
            per_contact_rules(find_disease(q).unwrap())
                .into_iter()
                .map(|r| r.label)
                .filter(|l| l.starts_with("Exclude"))
                .collect()
        };
        // Pertussis: only asymptomatic health care workers without
        // prophylaxis are excluded; the column must not read as "everyone".
        let p = labels("pertussis");
        assert_eq!(p.len(), 2, "{p:?}");
        for l in &p {
            assert!(l.contains("health care workers"), "{l}");
            assert!(l.contains("has no symptoms"), "{l}");
            assert!(l.contains("is not taking prophylaxis"), "{l}");
        }
        // Hepatitis A: susceptible food handlers without prophylaxis only.
        let h = labels("hepatitis A");
        assert!(!h.is_empty());
        for l in &h {
            assert!(l.contains("food handlers"), "{l}");
            assert!(l.contains("is not taking prophylaxis"), "{l}");
        }
        // Measles: the same days for every setting, so no setting is named.
        let m = labels("measles");
        assert_eq!(m.len(), 2, "{m:?}");
        for l in &m {
            assert!(l.contains("lacks evidence of immunity"), "{l}");
            assert!(!l.contains("school"), "{l}");
        }
    }

    #[test]
    fn missing_data_folder_is_a_note_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = plan_contact_tracing(
            &c,
            ContactTracingArgs {
                disease: "mumps".into(),
                onset_date: Some("2026-02-01".into()),
                ..ContactTracingArgs::default()
            },
        )
        .unwrap();
        assert!(env.data.template.is_none());
        assert!(env.caveats.iter().any(|c| c.contains("not written")));
        assert_eq!(col_letter(0), "A");
        assert_eq!(col_letter(27), "AB");
    }
}
