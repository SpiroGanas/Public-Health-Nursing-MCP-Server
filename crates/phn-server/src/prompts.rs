//! MCP prompts (reusable workflows the nurse can pick in the client).
//!
//! * `investigate_outbreak`: CDC's steps of a field investigation (CDC Field
//!   Epidemiology Manual, chapter 3), adapted to a Massachusetts local board of
//!   health working with DPH and MAVEN, calling the Phase 3 tools in order.

use rmcp::ErrorData;
use rmcp::model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role};

/// Name of the outbreak investigation prompt.
pub(crate) const INVESTIGATE_OUTBREAK: &str = "investigate_outbreak";

/// Every prompt the server offers.
pub(crate) fn list() -> Vec<Prompt> {
    let mut p = Prompt::new(
        INVESTIGATE_OUTBREAK,
        Some(
            "Step-by-step outbreak investigation for a Massachusetts board of health: CDC's \
             steps of a field investigation adapted to DPH and MAVEN reporting, calling this \
             server's communicable disease tools in order.",
        ),
        Some(vec![
            PromptArgument::new("disease")
                .with_title("Suspected disease or syndrome")
                .with_description(
                    "For example salmonellosis, norovirus, pertussis, or 'GI illness, agent \
                     unknown'.",
                )
                .with_required(false),
            PromptArgument::new("setting")
                .with_title("Setting")
                .with_description(
                    "Where the cases are: school, child care, long-term care, shelter, day \
                     program, restaurant or event, community.",
                )
                .with_required(false),
            PromptArgument::new("line_list_path")
                .with_title("Line list file")
                .with_description(
                    "A CSV or XLSX line list in the mounted data folder (identifiers stay in \
                     the file; tools return aggregates).",
                )
                .with_required(false),
            PromptArgument::new("exposure_datetime")
                .with_title("Known common exposure")
                .with_description("A meal or event date and time, YYYY-MM-DD HH:MM, if known.")
                .with_required(false),
        ]),
    );
    p.title = Some("Investigate an outbreak".into());
    vec![p]
}

fn arg(args: &Option<JsonObject>, name: &str) -> Option<String> {
    args.as_ref()
        .and_then(|a| a.get(name))
        .and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.trim().to_string()),
            serde_json::Value::Null => None,
            other => Some(other.to_string()),
        })
        .filter(|s| !s.is_empty())
}

/// Render a prompt.
pub(crate) fn get(name: &str, args: &Option<JsonObject>) -> Result<GetPromptResult, ErrorData> {
    if name != INVESTIGATE_OUTBREAK {
        return Err(ErrorData::invalid_params(
            format!("unknown prompt '{name}'; available: {INVESTIGATE_OUTBREAK}"),
            None,
        ));
    }
    let disease = arg(args, "disease");
    let setting = arg(args, "setting");
    let path = arg(args, "line_list_path");
    let exposure = arg(args, "exposure_datetime");
    for (n, v) in [
        ("disease", &disease),
        ("setting", &setting),
        ("line_list_path", &path),
        ("exposure_datetime", &exposure),
    ] {
        if v.as_ref().is_some_and(|s| s.len() > 300) {
            return Err(ErrorData::invalid_params(
                format!("argument '{n}' is too long"),
                None,
            ));
        }
    }
    let d = disease.clone().unwrap_or_else(|| "not yet known".into());
    let s = setting.clone().unwrap_or_else(|| "not stated".into());
    let ll = path
        .clone()
        .map(|p| format!("the line list '{p}'"))
        .unwrap_or_else(|| {
            "a line list (ask me to save one as CSV or XLSX in the data folder)".into()
        });
    let disease_arg = disease
        .as_ref()
        .map(|d| format!(", disease \"{d}\""))
        .unwrap_or_default();
    let exposure_arg = exposure
        .as_ref()
        .map(|e| format!(", exposure_datetime \"{e}\""))
        .unwrap_or_default();
    let path_arg = path
        .as_ref()
        .map(|p| format!("path \"{p}\""))
        .unwrap_or_else(|| "path to the line list".into());

    let text = format!(
        "Help me investigate a possible outbreak as a Massachusetts local public health nurse. \
Suspected disease: {d}. Setting: {s}. Data: {ll}.

Work through the steps of a field investigation (CDC Field Epidemiology Manual, chapter 3), \
adapted to a Massachusetts board of health. Several steps run at the same time; start control \
measures as soon as they are justified. At each step, call the tool named, show me the key \
result with its source and date, and tell me what you need from me next. Treat every answer \
as decision support: I decide, with DPH.

Privacy rules for the whole investigation: never ask me to paste names, dates of birth, \
addresses, phone numbers, or record numbers into the chat. Identifiable data stays in files \
in the data folder; the tools return aggregates with small-cell suppression and write \
identifiable output (contact lists, letters) only to the output folder.

1. Prepare and notify. Call lookup_reportable_condition for {d} to confirm how and how fast \
it must be reported, and remind me to call the DPH Division of Epidemiology (617-983-6800, \
24/7) early: DPH epidemiologists work with the board of health on clusters and outbreaks. \
Call search_ma_disease_control_guide{disease_arg} with no query to get the \
DPH guide chapter outline, then with queries such as \"outbreak\" and \"laboratory testing\".

2. Confirm the diagnosis. Ask what laboratory results exist; use \
search_ma_disease_control_guide (query \"laboratory testing\") for specimens and the State \
Public Health Laboratory.

3. Confirm that there is an outbreak. Compare the count with what is expected: if I have \
weekly or daily counts, call detect_aberrations; otherwise compare with past seasons in \
MAVEN reports. Watch for pseudo-outbreaks (new tests, duplicate reports, reporting changes).

4. Define and count cases. Help me write a case definition (clinical criteria, person, \
place, and time; confirmed, probable, suspect). Then call analyze_outbreak_line_list with \
{path_arg}{disease_arg}{exposure_arg}, using its case definition filters (case_values, \
onset_from, onset_to, require_any_symptom).

5. Describe time, place, and person. Call generate_epi_curve with the same file{disease_arg}\
{exposure_arg} and explain the pattern it finds (point source, continuous or intermittent \
common source, propagated) and its uncertainty. Summarize the person and place tables from \
step 4.

6. Start control measures now. For cases and ill food handlers, health care workers, \
students, and staff, call calculate_isolation_and_exclusion (setting: {s}). For the setting, \
call infection_control_for_congregate_settings (precautions, cohorting, PPE, EPA disinfectant \
list). If contacts need follow-up, call plan_contact_tracing (it writes a blank contact \
tracking workbook to the output folder). For measles, call assess_measles_exposure for each \
exposed person (no identifiers needed).

7. Develop and test hypotheses. Use the exposure-specific attack rates, risk ratios, and \
p-values from analyze_outbreak_line_list; check confounding between foods or exposures with \
analyze_two_by_two (stratified tables). Compare the probable exposure window with the event \
timeline.

8. Plan further studies if needed (a case-control study, environmental and food sampling \
with the health inspector, the State Public Health Laboratory). Use calculate_sample_size if \
a study is planned.

9. Implement and evaluate control measures. Re-run generate_epi_curve as new cases are \
added and look for secondary (person-to-person) cases one incubation period later; keep \
exclusions current with calculate_isolation_and_exclusion.

10. Communicate findings. Draft the situation summary for the board of health, notices for \
families or staff, and the final report, and export them with export_document (DOCX or PDF, \
accessibility checked). Make sure DPH has the line list and summary through MAVEN as DPH \
directs, and record the sources and dates of every rule used.

Begin with step 1 and ask me for what you need."
    );
    Ok(GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)])
        .with_description(format!(
            "Outbreak investigation steps for {d} in setting: {s}."
        )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_lists_and_renders_with_arguments() {
        let l = list();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].name, INVESTIGATE_OUTBREAK);
        let mut a = JsonObject::new();
        a.insert("disease".into(), "salmonellosis".into());
        a.insert("line_list_path".into(), "supper.csv".into());
        let r = get(INVESTIGATE_OUTBREAK, &Some(a)).unwrap();
        let text = r.messages[0].content.as_text().unwrap().text.clone();
        for tool in [
            "lookup_reportable_condition",
            "search_ma_disease_control_guide",
            "detect_aberrations",
            "analyze_outbreak_line_list",
            "generate_epi_curve",
            "calculate_isolation_and_exclusion",
            "infection_control_for_congregate_settings",
            "plan_contact_tracing",
            "export_document",
        ] {
            assert!(text.contains(tool), "{tool}");
        }
        assert!(text.contains("path \"supper.csv\""));
        assert!(get("nope", &None).is_err());
    }
}
