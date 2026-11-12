//! MCP prompts (reusable workflows the nurse can pick in the client).
//!
//! * `investigate_outbreak`: CDC's steps of a field investigation (CDC Field
//!   Epidemiology Manual, chapter 3), adapted to a Massachusetts local board of
//!   health working with DPH and MAVEN, calling the Phase 3 tools in order.
//! * `plan_tabletop_exercise`: an HSEEP exercise cycle (design, conduct,
//!   after-action report and improvement plan) for a Massachusetts local health
//!   department, calling the Phase 12 and 13 preparedness tools in order.

use phn_core::Toolset;
use rmcp::ErrorData;
use rmcp::model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role};

/// Name of the outbreak investigation prompt.
pub(crate) const INVESTIGATE_OUTBREAK: &str = "investigate_outbreak";

/// Name of the tabletop exercise prompt (Phase 13, Pilot 3).
pub(crate) const PLAN_TABLETOP_EXERCISE: &str = "plan_tabletop_exercise";

/// Tool groups the outbreak prompt's tools belong to (`core` is always on).
const OUTBREAK_TOOLSETS: &[Toolset] = &[Toolset::Cd, Toolset::Epi];

/// Tool groups the tabletop prompt's tools belong to (`core` is always on).
const TABLETOP_TOOLSETS: &[Toolset] = &[
    Toolset::Epr,
    Toolset::Equity,
    Toolset::Env,
    Toolset::Cd,
    Toolset::Vax,
];

// Phase 15 cha prompt (community_health_assessment) constants go below this line.
/// Name of the MAPP 2.0 community health assessment prompt (Phase 15).
pub(crate) const COMMUNITY_HEALTH_ASSESSMENT: &str = "community_health_assessment";

/// Tool groups the community health assessment prompt's tools belong to (`core` is always on).
const CHA_TOOLSETS: &[Toolset] = &[Toolset::Equity, Toolset::Chronic, Toolset::Env];
// End of Phase 15 cha prompt constants.

/// Tells the model which tool groups a workflow uses and what to do when one
/// of them is off (prompts are listed whatever toolsets are enabled).
fn toolset_note(needed: &[Toolset]) -> String {
    let names: Vec<&str> = needed.iter().map(|t| t.as_str()).collect();
    format!(
        "Tool groups: this workflow uses the {} toolsets (core tools are always on). If a tool \
named below is missing from your tool list, or a call fails because its toolset is not enabled, \
do not retry it: tell me which toolset is off and which step it affects, and carry on with the \
steps that do not need it. I can turn a toolset on by asking you to save it with \
configure_jurisdiction (toolsets); if the error says the toolsets are fixed by PHN_PROFILE or \
PHN_TOOLSETS, I have to change that in the client configuration instead.",
        names.join(", ")
    )
}

/// Values of `calculate_isolation_and_exclusion`'s `settings` argument that fit
/// a free-text outbreak setting, as a starting point for the model. Child care
/// and school have their own rule sets; elsewhere `other` covers residents,
/// guests, and the public.
fn isolation_settings(setting: &str) -> Vec<&'static str> {
    let s = setting.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| s.contains(*w));
    let mut out = Vec::new();
    if !s.contains("adult day")
        && has(&[
            "child care",
            "childcare",
            "child-care",
            "day care",
            "daycare",
            "preschool",
            "pre-school",
        ])
    {
        out.push("child_care");
    }
    if s.contains("school") {
        out.push("school");
    }
    if has(&["restaurant", "food", "cafeteria", "kitchen", "catering"]) {
        out.push("food_handler");
    }
    if has(&[
        "long-term care",
        "long term care",
        "nursing home",
        "hospital",
        "health care",
        "healthcare",
        "clinic",
    ]) {
        out.push("health_care_worker");
    }
    if !out.contains(&"child_care") && !out.contains(&"school") {
        out.push("other");
    }
    out
}

/// Every prompt the server offers.
pub(crate) fn list() -> Vec<Prompt> {
    let mut p = Prompt::new(
        INVESTIGATE_OUTBREAK,
        Some(
            "Step-by-step outbreak investigation for a Massachusetts board of health: CDC's \
             steps of a field investigation adapted to DPH and MAVEN reporting, calling this \
             server's communicable disease tools in order. Uses the cd and epi toolsets.",
        ),
        Some(vec![
            PromptArgument::new("disease")
                .with_title("Suspected disease or syndrome")
                .with_description(
                    "For example salmonellosis, norovirus, or pertussis. Leave it empty when \
                     the agent is not known yet.",
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
             continuity plan), the hotwash, and the after-action report and improvement plan. \
             Uses the epr toolset; equity, env, cd, and vax tools add context when enabled.",
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
    let mut all = vec![p, t];
    // Phase 15 cha prompts go below this line.
    all.push(community_health_assessment_prompt());
    // End of Phase 15 cha prompts.
    all
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
        // Phase 15 cha prompt arm goes below this line.
        COMMUNITY_HEALTH_ASSESSMENT => community_health_assessment(args),
        // End of Phase 15 cha prompt arm.
        _ => Err(ErrorData::invalid_params(
            format!(
                "unknown prompt '{name}'; available: {INVESTIGATE_OUTBREAK}, \
                 {PLAN_TABLETOP_EXERCISE}, {COMMUNITY_HEALTH_ASSESSMENT}"
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
    // With no agent yet, the model must not pass a syndrome as a disease name.
    let unknown_note = if disease.is_some() {
        ""
    } else {
        "\n\nThe agent is not known yet. Do not pass a syndrome description (such as \"GI \
illness\") as a disease name: leave the disease argument out of every tool call until laboratory \
results or the clinical picture point to an agent, and tell me which answers are missing because \
of that."
    };
    let step1 = if disease.is_some() {
        format!(
            "Call lookup_reportable_condition for {d} to confirm how and how fast \
it must be reported, and remind me to call the DPH Division of Epidemiology (617-983-6800, \
24/7) early: DPH epidemiologists work with the board of health on clusters and outbreaks. \
Call search_ma_disease_control_guide{disease_arg} with no query to get the \
DPH guide chapter outline, then with queries such as \"outbreak\" and \"laboratory testing\"."
        )
    } else {
        "Remind me to call the DPH Division of Epidemiology (617-983-6800, 24/7) early: DPH \
epidemiologists work with the board of health on clusters and outbreaks. As soon as an agent is \
suspected, call lookup_reportable_condition for it to confirm how and how fast it must be \
reported, and call search_ma_disease_control_guide with that disease and no query to get the DPH \
guide chapter outline, then with queries such as \"outbreak\" and \"laboratory testing\"."
            .to_string()
    };
    // calculate_isolation_and_exclusion and infection_control_for_congregate_settings
    // both need a disease or pathogen name.
    let (iso_when, ic_when) = if disease.is_some() {
        (format!(" (disease \"{d}\")"), "")
    } else {
        (
            " once an agent is suspected (it needs a disease name)".to_string(),
            " once a pathogen is suspected",
        )
    };
    let settings_hint = match &setting {
        Some(text) => format!(
            "For the setting I gave ({text}), start with settings [{}] and adjust it to who is \
ill.",
            isolation_settings(text)
                .iter()
                .map(|v| format!("\"{v}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        None => "I have not told you the setting: ask me, or leave settings out to see the \
rules for all five."
            .to_string(),
    };
    let toolsets = toolset_note(OUTBREAK_TOOLSETS);

    let text = format!(
        "Help me investigate a possible outbreak as a Massachusetts local public health nurse. \
Suspected disease: {d}. Setting: {s}. Data: {ll}.{unknown_note}

Work through the steps of a field investigation (CDC Field Epidemiology Manual, chapter 3), \
adapted to a Massachusetts board of health. Several steps run at the same time; start control \
measures as soon as they are justified. At each step, call the tool named, show me the key \
result with its source and date, and tell me what you need from me next. Treat every answer \
as decision support: I decide, with DPH.

Privacy rules for the whole investigation: never ask me to paste names, dates of birth, \
addresses, phone numbers, or record numbers into the chat. Identifiable data stays in files \
in the data folder; the tools return aggregates with small-cell suppression and write \
identifiable output (contact lists, letters) only to the output folder.

{toolsets}

1. Prepare and notify. {step1}

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
students, and staff, call calculate_isolation_and_exclusion{iso_when}. Its settings argument is \
a list of one or more of child_care, school, food_handler, health_care_worker, other (leave it \
out for all five); pick every value that describes the person, for example school and \
food_handler for an ill cafeteria worker at a school. {settings_hint} For the setting, \
call infection_control_for_congregate_settings (precautions, cohorting, PPE, EPA disinfectant \
list){ic_when}. If contacts need follow-up, call plan_contact_tracing (it writes a blank contact \
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
    let toolsets = toolset_note(TABLETOP_TOOLSETS);

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

{toolsets} Only the epr toolset is essential here; the equity, env, cd, and vax tools add context \
for some scenarios, and the preparedness profile does not include vax \
(analyze_vaccination_coverage, plan_vaccination_clinic).

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

// Phase 15 cha prompt (community_health_assessment) goes below this line.
/// The prompt's listing: a MAPP 2.0 community health assessment workflow.
fn community_health_assessment_prompt() -> Prompt {
    let mut p = Prompt::new(
        COMMUNITY_HEALTH_ASSESSMENT,
        Some(
            "A MAPP 2.0 community health assessment for a Massachusetts town or county: build the \
             foundation and partner list, assemble the community status data (profile, social \
             vulnerability, language, chronic disease, disparities, environment, food, water), \
             add the community context from focus groups and interviews (names removed locally), \
             bring in nearby hospitals' CHNAs, then rank the issues with the Hanlon method, PEARL, \
             and a nominal group vote. Uses the equity toolset; chronic and env tools add data when \
             enabled.",
        ),
        Some(vec![
            PromptArgument::new("area")
                .with_title("Town or county")
                .with_description("The Massachusetts city, town, or county to assess.")
                .with_required(false),
            PromptArgument::new("partners_path")
                .with_title("Partner roster file")
                .with_description(
                    "A CSV or XLSX in the data folder with one partner organization per row \
                     (organization name and sector). Contact people are never read.",
                )
                .with_required(false),
            PromptArgument::new("chna_folder")
                .with_title("Folder of hospital CHNAs")
                .with_description(
                    "A folder in the data folder with nearby hospitals' community health needs \
                     assessment reports (PDF or Word).",
                )
                .with_required(false),
            PromptArgument::new("transcripts_folder")
                .with_title("Folder of focus group and interview transcripts")
                .with_description(
                    "A folder in the data folder with transcripts. They are read only on this \
                     machine; only theme counts come back.",
                )
                .with_required(false),
        ]),
    );
    p.title = Some("MAPP 2.0 community health assessment".into());
    p
}

fn community_health_assessment(args: &Option<JsonObject>) -> Result<GetPromptResult, ErrorData> {
    let area = arg(args, "area");
    let partners = arg(args, "partners_path");
    let chna = arg(args, "chna_folder");
    let transcripts = arg(args, "transcripts_folder");
    check_lengths(&[
        ("area", &area),
        ("partners_path", &partners),
        ("chna_folder", &chna),
        ("transcripts_folder", &transcripts),
    ])?;
    let ar = area
        .clone()
        .unwrap_or_else(|| "my saved town (ask me if none is saved)".into());
    let area_arg = area
        .as_ref()
        .map(|a| format!("area \"{a}\""))
        .unwrap_or_else(|| "the area".into());
    let partners_arg = partners
        .as_ref()
        .map(|p| format!(", partners_path \"{p}\""))
        .unwrap_or_default();
    let chna_arg = chna
        .as_ref()
        .map(|c| format!(", chna_paths [\"{c}\"]"))
        .unwrap_or_default();
    let chna_text = match &chna {
        Some(c) => format!("Pass chna_paths [\"{c}\"] so the tool reads the priority lists."),
        None => "Ask me to save each nearby hospital's CHNA (PDF or Word) in a folder in the data \
folder, such as /data/chna, and then call it again with chna_paths."
            .into(),
    };
    let transcripts_text = match &transcripts {
        Some(t) => format!("Call analyze_qualitative_data (EQ-10) with transcript_paths [\"{t}\"]."),
        None => "If I have focus group or interview transcripts, ask me to save them in a folder \
in the data folder and call analyze_qualitative_data (EQ-10) with transcript_paths."
            .into(),
    };
    let toolsets = toolset_note(CHA_TOOLSETS);

    let text = format!(
        "Help me carry out a community health assessment as a Massachusetts local public health \
nurse, following MAPP 2.0 (Mobilizing for Action through Planning and Partnerships, NACCHO). \
Area: {ar}.

At each step, call the tool named, show me the key result with its source and date, and tell me \
what you need from me next. Treat the results as decision support: I decide with my partners. \
Be plain about what the tools could not verify: MAPP 2.0 was checked against journal articles \
and not NACCHO's own handbook, so tell me to check NACCHO's materials before citing a phase or \
tool as NACCHO's wording.

Privacy rules for the whole assessment: never ask me to paste transcripts, names, or client \
details into the chat. Transcripts and client files stay in files in the data folder; the tools \
read them locally and return counts, with small counts withheld, and write anything detailed \
only to the output folder.

{toolsets}

1. Build the foundation (MAPP 2.0 Phase 1). Ask who leads the assessment, who the partners are, \
and who is missing. Call community_health_assessment_builder (EQ-08) with {area_arg}{partners_arg}{chna_arg} \
to get the three phases and three assessments, the partner sector checklist, nearby hospitals, \
and the first data sections. Ask me to save a partner roster (organization and sector) in the \
data folder if I have none. Call find_community_resources (EQ-07) to inventory local services \
and note the gaps.

2. Community status assessment (quantitative data). Go through the data sections the builder \
returned and call the underlying tools when I want more detail: build_community_health_profile \
(EQ-01), get_social_vulnerability_index (EQ-02), language_access_profile (EQ-03), \
get_places_estimates (CHR-01) for chronic disease, environmental_justice_screen, \
climate_health_vulnerability_profile, food_access_mapper, drinking_water_quality_lookup, and \
community_water_fluoridation_status. If I have a table of counts by race, ethnicity, language, \
or income, call measure_health_disparities (EQ-04); ask me to save it in the data folder. If I \
have a screening file of health-related social needs, call screen_health_related_social_needs \
(EQ-05) for aggregates only. Use estimate_benefit_eligibility (EQ-06) only for teaching \
examples of which programs residents may qualify for, not for individual clients. Flag small \
numbers, unreliable estimates, and data older than five years.

3. Community context assessment (qualitative data). Review the focus group and interview \
prompts the builder returned. {transcripts_text} Report the themes it returns (participant \
counts only) and remind me that keyword coding is a heuristic: I should read the coded excerpts \
in the output folder myself before reporting a finding.

4. Hospital CHNAs. Show the hospitals near the area and the links to find their community \
health needs assessments (the Attorney General's community benefits reports). {chna_text} \
Compare the hospitals' priorities with the data and the context themes: where do they agree, \
and what is missing?

5. Bring the story together. Help me list the themes that appear across the status data, the \
context assessment, the partner assessment, and the CHNAs. Propose a list of candidate health \
issues (do not rank them yet) and ask me to confirm it.

6. Prioritize. For the confirmed issues, walk me through the Hanlon method and call \
prioritize_health_issues (EQ-09): ask me for each issue's size (0-10, from the data), \
seriousness (0-20), effectiveness of an available intervention (0-10), and the five PEARL \
answers (propriety, economics, acceptability, resources, legality), and write down the \
rationale for each. If my partners held a nominal group vote, enter the totals only (how many \
participants gave each issue each rank), never names. Show the ranking, the ties, the close \
calls, and the strategy grid; do not break ties for me.

7. Report. Write the draft sections with community_health_assessment_builder (the document goes \
to the output folder), then list what is still missing (data gaps, partners, voices) and what \
should be collected next. The next phase of the work, planning and evaluation, follows from \
the priorities.

Begin with step 1 and ask me for what you need."
    );
    Ok(
        GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)])
            .with_description(format!("MAPP 2.0 community health assessment for {ar}.")),
    )
}
// End of Phase 15 cha prompt.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_lists_and_renders_with_arguments() {
        let l = list();
        assert!(l.len() >= 2, "later phases add prompts");
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

    fn render(name: &str, pairs: &[(&str, &str)]) -> String {
        let mut a = JsonObject::new();
        for (k, v) in pairs {
            a.insert((*k).into(), (*v).into());
        }
        let r = get(name, &Some(a)).unwrap();
        r.messages[0].content.as_text().unwrap().text.clone()
    }

    #[test]
    fn outbreak_prompt_with_no_agent_does_not_offer_a_syndrome_as_a_disease() {
        // The argument description no longer suggests a value no tool accepts.
        let l = list();
        let disease_arg = l[0]
            .arguments
            .as_ref()
            .unwrap()
            .iter()
            .find(|a| a.name == "disease")
            .unwrap();
        let desc = disease_arg.description.as_deref().unwrap();
        assert!(!desc.contains("agent unknown'"), "{desc}");
        assert!(desc.contains("Leave it empty"), "{desc}");

        let text = render(INVESTIGATE_OUTBREAK, &[("setting", "school")]);
        assert!(text.contains("leave the disease argument out of every tool call"));
        assert!(!text.contains("lookup_reportable_condition for not yet known"));
        assert!(!text.contains("disease \"not yet known\""));
        // The tools that need a disease are deferred, not called with a guess.
        assert!(text.contains(
            "calculate_isolation_and_exclusion once an agent is suspected (it needs a disease name)"
        ));
        // With a disease the note is absent and the disease is passed.
        let known = render(INVESTIGATE_OUTBREAK, &[("disease", "pertussis")]);
        assert!(!known.contains("The agent is not known yet"));
        assert!(known.contains("lookup_reportable_condition for pertussis"));
        assert!(known.contains("calculate_isolation_and_exclusion (disease \"pertussis\")"));
    }

    #[test]
    fn outbreak_prompt_uses_the_real_settings_argument() {
        // `calculate_isolation_and_exclusion` has `settings` (a list), not `setting`.
        for (setting, expect) in [
            ("school", "settings [\"school\"]"),
            ("child care", "settings [\"child_care\"]"),
            (
                "long-term care",
                "settings [\"health_care_worker\", \"other\"]",
            ),
            (
                "restaurant or event",
                "settings [\"food_handler\", \"other\"]",
            ),
            ("shelter", "settings [\"other\"]"),
            ("adult day health program", "settings [\"other\"]"),
            (
                "School cafeteria",
                "settings [\"school\", \"food_handler\"]",
            ),
        ] {
            let text = render(
                INVESTIGATE_OUTBREAK,
                &[("disease", "norovirus"), ("setting", setting)],
            );
            assert!(text.contains(expect), "{setting}: expected {expect}");
            assert!(!text.contains("(setting: "), "{setting}");
            assert!(text.contains("child_care, school, food_handler, health_care_worker, other"));
        }
        let none = render(INVESTIGATE_OUTBREAK, &[("disease", "norovirus")]);
        assert!(none.contains("leave settings out to see the rules for all five"));
        // Every suggested value is one the tool accepts.
        for v in isolation_settings("school, child care, hospital kitchen, community") {
            assert!(
                [
                    "child_care",
                    "school",
                    "food_handler",
                    "health_care_worker",
                    "other"
                ]
                .contains(&v),
                "{v}"
            );
        }
    }

    #[test]
    fn prompts_state_the_toolsets_their_tools_need() {
        for (name, needed) in [
            (INVESTIGATE_OUTBREAK, OUTBREAK_TOOLSETS),
            (PLAN_TABLETOP_EXERCISE, TABLETOP_TOOLSETS),
        ] {
            let text = render(name, &[]);
            assert!(text.contains(&toolset_note(needed)), "{name}");
            assert!(text.contains("do not retry it"), "{name}");
            // Every tool the prompt names is in `core` or in a toolset it declares.
            let mut named = 0;
            for t in phn_tools::registry::tools() {
                if text.contains(t.name) {
                    named += 1;
                    assert!(
                        t.toolset == Toolset::Core || needed.contains(&t.toolset),
                        "{name} names {} ({}) but does not declare that toolset",
                        t.name,
                        t.toolset
                    );
                }
            }
            assert!(named >= 9, "{name} names {named} tools");
            // Every declared toolset is used by at least one named tool.
            for ts in needed {
                assert!(
                    phn_tools::registry::tools().any(|t| t.toolset == *ts && text.contains(t.name)),
                    "{name} declares {ts} but names no tool from it"
                );
            }
        }
        let outbreak = render(INVESTIGATE_OUTBREAK, &[]);
        assert!(outbreak.contains("uses the cd, epi toolsets"));
        let tabletop = render(PLAN_TABLETOP_EXERCISE, &[]);
        assert!(tabletop.contains("the preparedness profile does not include vax"));
    }
    // Phase 15 cha prompt tests go below this line.
    #[test]
    fn community_health_assessment_prompt_chains_the_equity_tools() {
        assert!(list().iter().any(|p| p.name == COMMUNITY_HEALTH_ASSESSMENT));
        let text = render(
            COMMUNITY_HEALTH_ASSESSMENT,
            &[
                ("area", "Lowell"),
                ("chna_folder", "chna"),
                ("transcripts_folder", "focus_groups"),
                ("partners_path", "partners.csv"),
            ],
        );
        for tool in [
            "community_health_assessment_builder",
            "build_community_health_profile",
            "get_social_vulnerability_index",
            "language_access_profile",
            "measure_health_disparities",
            "screen_health_related_social_needs",
            "estimate_benefit_eligibility",
            "find_community_resources",
            "analyze_qualitative_data",
            "prioritize_health_issues",
            "get_places_estimates",
        ] {
            assert!(text.contains(tool), "{tool}");
        }
        // EQ-01 to EQ-10 are all named.
        for id in ["EQ-01", "EQ-02", "EQ-03", "EQ-04", "EQ-05", "EQ-06", "EQ-07", "EQ-08", "EQ-09", "EQ-10"] {
            assert!(text.contains(id), "{id}");
        }
        assert!(text.contains("area \"Lowell\"") && text.contains("partners_path \"partners.csv\""));
        assert!(text.contains("chna_paths [\"chna\"]"));
        assert!(text.contains("transcript_paths [\"focus_groups\"]"));
        assert!(text.contains("not NACCHO's own handbook") || text.contains("not NACCHO's own"));
        // Every tool named is registered and in core or a declared toolset.
        let mut named = 0;
        for t in phn_tools::registry::tools() {
            if text.contains(t.name) {
                named += 1;
                assert!(
                    t.toolset == Toolset::Core || CHA_TOOLSETS.contains(&t.toolset),
                    "{} ({}) is not in the declared toolsets",
                    t.name,
                    t.toolset
                );
            }
        }
        assert!(named >= 10, "{named}");
        for ts in CHA_TOOLSETS {
            assert!(
                phn_tools::registry::tools().any(|t| t.toolset == *ts && text.contains(t.name)),
                "{ts} is declared but no tool from it is named"
            );
        }
        assert!(text.contains(&toolset_note(CHA_TOOLSETS)));
        // Without arguments it asks for what it needs.
        let bare = render(COMMUNITY_HEALTH_ASSESSMENT, &[]);
        assert!(bare.contains("my saved town") && bare.contains("save each nearby hospital's CHNA"));
        let mut long = JsonObject::new();
        long.insert("area".into(), "x".repeat(301).into());
        assert!(get(COMMUNITY_HEALTH_ASSESSMENT, &Some(long)).is_err());
    }
    // End of Phase 15 cha prompt tests.
}
