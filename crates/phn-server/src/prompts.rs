//! MCP prompts (reusable workflows the nurse can pick in the client).
//!
//! * `investigate_outbreak`: CDC's steps of a field investigation (CDC Field
//!   Epidemiology Manual, chapter 3), adapted to a Massachusetts local board of
//!   health working with DPH and MAVEN, calling the Phase 3 tools in order.
//! * `plan_tabletop_exercise`: an HSEEP exercise cycle (design, conduct,
//!   after-action report and improvement plan) for a Massachusetts local health
//!   department, calling the Phase 12 and 13 preparedness tools in order.

use rmcp::ErrorData;
use rmcp::model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role};

/// Name of the outbreak investigation prompt.
pub(crate) const INVESTIGATE_OUTBREAK: &str = "investigate_outbreak";

/// Name of the tabletop exercise prompt (Phase 13, Pilot 3).
pub(crate) const PLAN_TABLETOP_EXERCISE: &str = "plan_tabletop_exercise";

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
    let mut t = Prompt::new(
        PLAN_TABLETOP_EXERCISE,
        Some(
            "Plan, run, and evaluate an HSEEP tabletop exercise for a Massachusetts local \
             health department: hazard context, exercise design with objectives mapped to CDC \
             PHEP and FEMA core capabilities, player materials (ICS forms, crisis messages, \
             continuity plan), the hotwash, and the after-action report and improvement plan.",
        ),
        Some(vec![
            PromptArgument::new("scenario")
                .with_title("Scenario")
                .with_description(
                    "For example measles outbreak, coastal storm with power outage, extreme \
                     heat, or another hazard.",
                )
                .with_required(false),
            PromptArgument::new("area")
                .with_title("Town or county")
                .with_description("The Massachusetts city, town, or county the exercise covers.")
                .with_required(false),
            PromptArgument::new("exercise_date")
                .with_title("Exercise date")
                .with_description("Planned date, YYYY-MM-DD.")
                .with_required(false),
            PromptArgument::new("roster_path")
                .with_title("Staff roster file")
                .with_description(
                    "A CSV or XLSX staff roster in the data folder (names, positions, phones, \
                     emails). Contacts go only into documents in the output folder.",
                )
                .with_required(false),
        ]),
    );
    t.title = Some("Plan a tabletop exercise".into());
    vec![p, t]
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

/// Reject over-long prompt arguments.
fn check_lengths(values: &[(&str, &Option<String>)]) -> Result<(), ErrorData> {
    for (n, v) in values {
        if v.as_ref().is_some_and(|s| s.len() > 300) {
            return Err(ErrorData::invalid_params(
                format!("argument '{n}' is too long"),
                None,
            ));
        }
    }
    Ok(())
}

/// Render a prompt.
pub(crate) fn get(name: &str, args: &Option<JsonObject>) -> Result<GetPromptResult, ErrorData> {
    match name {
        INVESTIGATE_OUTBREAK => outbreak(args),
        PLAN_TABLETOP_EXERCISE => tabletop(args),
        _ => Err(ErrorData::invalid_params(
            format!(
                "unknown prompt '{name}'; available: {INVESTIGATE_OUTBREAK}, \
                 {PLAN_TABLETOP_EXERCISE}"
            ),
            None,
        )),
    }
}

fn outbreak(args: &Option<JsonObject>) -> Result<GetPromptResult, ErrorData> {
    let disease = arg(args, "disease");
    let setting = arg(args, "setting");
    let path = arg(args, "line_list_path");
    let exposure = arg(args, "exposure_datetime");
    check_lengths(&[
        ("disease", &disease),
        ("setting", &setting),
        ("line_list_path", &path),
        ("exposure_datetime", &exposure),
    ])?;
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
    Ok(
        GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)]).with_description(
            format!("Outbreak investigation steps for {d} in setting: {s}."),
        ),
    )
}

fn tabletop(args: &Option<JsonObject>) -> Result<GetPromptResult, ErrorData> {
    let scenario = arg(args, "scenario");
    let area = arg(args, "area");
    let date = arg(args, "exercise_date");
    let roster = arg(args, "roster_path");
    check_lengths(&[
        ("scenario", &scenario),
        ("area", &area),
        ("exercise_date", &date),
        ("roster_path", &roster),
    ])?;
    let sc = scenario
        .clone()
        .unwrap_or_else(|| "not chosen yet (help me choose)".into());
    let ar = area
        .clone()
        .unwrap_or_else(|| "my saved town (ask me if none is saved)".into());
    let dt = date.clone().unwrap_or_else(|| "not set".into());
    let roster_text = roster
        .as_ref()
        .map(|r| {
            format!(
                "Use the staff roster '{r}' (roster_path) wherever a tool accepts one, so phone \
numbers and emails go only into the documents in the output folder."
            )
        })
        .unwrap_or_else(|| {
            "If I want staff contacts in the documents, ask me to save a staff roster (CSV or \
XLSX with name, position, phone, and email columns) in the data folder and pass it as \
roster_path."
                .into()
        });

    let text = format!(
        "Help me plan, run, and evaluate a discussion-based tabletop exercise as a Massachusetts \
local public health nurse, following the Homeland Security Exercise and Evaluation Program \
(HSEEP). Scenario: {sc}. Area: {ar}. Exercise date: {dt}.

At each step, call the tool named, show me the key result with its source and date, and tell \
me what you need from me next. The exercise scenario is fictional; label it that way in every \
document. Treat clinical, legal, and planning content as decision support: I decide, with my \
agency, DPH, my HMCC, and MEMA.

Privacy rules: staff names and positions may be used in the chat, but never ask me to paste \
phone numbers, emails, or anything about clients or patients. {roster_text} If notes from the \
hotwash mention clients or staff details, help me remove them before they go into a tool.

1. Choose the hazard and set the context. If the scenario is not chosen, call \
hazard_vulnerability_assessment for the area and suggest a scenario from the top hazards; \
for a storm or heat scenario, call get_active_hazard_alerts with guidance_for the hazard. \
Describe the people most affected: call build_community_health_profile and \
language_access_profile for the area; for a storm or power outage, also \
get_electricity_dependent_population; for heat, heat_health_action_planner; for measles, \
search_ma_disease_control_guide (measles) and analyze_vaccination_coverage if I have school \
data.

2. Design the exercise. Call design_hseep_exercise (exercise type tabletop) with the \
scenario, the area, the participating organizations and player roles I give you, and three \
to five SMART objectives, each mapped to CDC PHEP capabilities and FEMA core capabilities. \
Review the objectives with me before writing the documents (situation manual, facilitator \
guide, exercise evaluation guides, feedback form, and sign-in sheet go to the output folder).

3. Prepare player materials that the modules will use. Call generate_ics_forms for the \
exercise's first operational period (an Incident Action Plan with the 202 objectives and \
the 203/207 organization; blank forms where players will fill them in). Call \
crisis_risk_communication_builder for the scenario's initial and maintenance phases \
(message maps, holding statement, FAQs, and the area's priority languages). If an inject \
involves staff absences or a closed building, call continuity_of_operations_planner. Use \
model_dispensing_site_throughput or plan_vaccination_clinic for a clinic or POD inject, and \
functional_needs_shelter_intake for a shelter inject.

4. Conduct and hotwash. Give me a facilitator run sheet from the situation manual, then help \
me capture strengths and areas for improvement for each objective during the hotwash.

5. Write the after-action report. Call build_after_action_report with the objectives, \
capabilities, observations (strengths and areas for improvement with analysis and root \
causes), and corrective actions (POETE element, responsible organization, point of contact, \
start and completion dates). If participants' feedback-form ratings are in a file in the \
data folder, pass it so the ratings are totaled locally. Then help me schedule the \
corrective actions and the next exercise.

Begin with step 1 and ask me for what you need."
    );
    Ok(
        GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)])
            .with_description(format!("HSEEP tabletop exercise cycle for {sc} in {ar}.")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_lists_and_renders_with_arguments() {
        let l = list();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].name, INVESTIGATE_OUTBREAK);
        assert_eq!(l[1].name, PLAN_TABLETOP_EXERCISE);
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

    #[test]
    fn tabletop_prompt_names_the_preparedness_tools() {
        let mut a = JsonObject::new();
        a.insert("scenario".into(), "measles outbreak".into());
        a.insert("roster_path".into(), "staff_roster.csv".into());
        let r = get(PLAN_TABLETOP_EXERCISE, &Some(a)).unwrap();
        let text = r.messages[0].content.as_text().unwrap().text.clone();
        for tool in [
            "hazard_vulnerability_assessment",
            "design_hseep_exercise",
            "generate_ics_forms",
            "crisis_risk_communication_builder",
            "continuity_of_operations_planner",
            "build_after_action_report",
        ] {
            assert!(text.contains(tool), "{tool}");
        }
        assert!(text.contains("measles outbreak") && text.contains("'staff_roster.csv'"));
        let mut long = JsonObject::new();
        long.insert("area".into(), "x".repeat(301).into());
        assert!(get(PLAN_TABLETOP_EXERCISE, &Some(long)).is_err());
    }
}
