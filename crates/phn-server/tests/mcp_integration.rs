//! MCP integration tests: every tool through a real MCP client.
//!
//! Covers the Phase 0 and Phase 1 exit criteria that can be automated:
//! schema validity and annotations for every tool, tool-level errors,
//! toolset filtering, the PHI detector against the synthetic corpus, no
//! row-level data by default, and exported documents passing the server's own
//! accessibility checks.

mod common;

use std::collections::BTreeSet;

use common::{everything, leaks, start, summary};
use serde_json::json;

/// Every tool in the registry (the server must list exactly these).
fn all_tools() -> BTreeSet<String> {
    phn_tools::registry::tools()
        .map(|t| t.name.to_string())
        .collect()
}

#[tokio::test]
async fn every_tool_has_schemas_annotations_and_a_description() {
    let h = start(|_| {}).await;
    let tools = h.client.list_all_tools().await.unwrap();
    let names: BTreeSet<String> = tools.iter().map(|t| t.name.to_string()).collect();
    assert_eq!(names, all_tools());
    assert!(names.len() >= 11);
    for t in &tools {
        let desc = t.description.as_deref().unwrap_or("");
        assert!(
            desc.len() > 80,
            "{} needs a description written for the model",
            t.name
        );
        assert_eq!(
            t.input_schema.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "{}",
            t.name
        );
        let out = t
            .output_schema
            .as_ref()
            .unwrap_or_else(|| panic!("{} has no output schema", t.name));
        let props = out
            .get("properties")
            .and_then(|p| p.as_object())
            .expect("object output schema");
        for key in ["summary", "data", "sources", "caveats", "privacy"] {
            assert!(props.contains_key(key), "{} output lacks {key}", t.name);
        }
        let a = t
            .annotations
            .as_ref()
            .unwrap_or_else(|| panic!("{} has no annotations", t.name));
        assert!(
            a.title.is_some() && a.read_only_hint.is_some(),
            "{} annotations incomplete",
            t.name
        );
        // Input and output schemas must themselves be valid JSON Schema.
        jsonschema::validator_for(&serde_json::Value::Object((*t.input_schema).clone()))
            .unwrap_or_else(|e| panic!("{} input schema invalid: {e}", t.name));
        jsonschema::validator_for(&serde_json::Value::Object((**out).clone()))
            .unwrap_or_else(|e| panic!("{} output schema invalid: {e}", t.name));
        assert!(
            phn_tools::registry::tool(&t.name).is_some(),
            "{} missing from registry",
            t.name
        );
    }
}

#[tokio::test]
async fn successful_calls_return_markdown_and_schema_valid_structured_content() {
    let h = start(|_| {}).await;
    let tools = h.client.list_all_tools().await.unwrap();
    let calls = vec![
        ("get_data_source_status", json!({"include_planned": true})),
        ("configure_jurisdiction", json!({"town": "Amherst"})),
        (
            "calculate_rates",
            json!({"groups": [{"label": "Amherst", "count": 12, "population": 39263}, {"label": "Hadley", "count": 3, "population": 5325}]}),
        ),
        ("lookup_reportable_condition", json!({"query": "measles"})),
        ("read_local_file", json!({})),
        ("read_local_file", json!({"path": "line_list.csv"})),
        (
            "read_local_file",
            json!({"path": "policies/tb_screening.txt"}),
        ),
        (
            "search_agency_policies",
            json!({"query": "needlestick supervisor"}),
        ),
        (
            "export_document",
            json!({"title": "Test export", "content_markdown": "# Heading\n\nBody text.\n\n| A | B |\n|---|---|\n| 1 | 2 |", "formats": ["docx", "pdf", "xlsx", "csv", "svg", "png"]}),
        ),
        (
            "apply_small_cell_suppression",
            json!({"path": "counts_by_town.csv"}),
        ),
        (
            "profile_local_dataset",
            json!({"path": "line_list.csv", "include_value_summaries": true}),
        ),
        (
            "deidentify_dataset",
            json!({"path": "line_list.csv", "reference_date": "2026-11-01"}),
        ),
        (
            "check_document_accessibility",
            json!({"path": "policies/needlestick_exposure.md"}),
        ),
    ];
    for (name, args) in calls {
        let r = h.call(name, args.clone()).await;
        if name == "check_document_accessibility" {
            // Markdown is not a supported format: a tool-level error, checked below.
            assert_eq!(r.is_error, Some(true));
            continue;
        }
        assert_eq!(r.is_error, Some(false), "{name} {args}: {}", summary(&r));
        let md = summary(&r);
        assert!(
            !md.trim().is_empty() && !md.trim_start().starts_with('{'),
            "{name}: first block should be markdown"
        );
        let structured = r.structured_content.clone().expect("structured content");
        let tool = tools.iter().find(|t| t.name == name).unwrap();
        let schema = serde_json::Value::Object((**tool.output_schema.as_ref().unwrap()).clone());
        let v = jsonschema::validator_for(&schema).unwrap();
        let errors: Vec<String> = v
            .iter_errors(&structured)
            .map(|e| format!("{e} at {}", e.instance_path()))
            .take(5)
            .collect();
        assert!(
            errors.is_empty(),
            "{name} output does not match its schema: {errors:#?}"
        );
    }
}

#[tokio::test]
async fn bad_input_is_a_tool_error_with_a_helpful_message() {
    let h = start(|_| {}).await;
    let cases = vec![
        (
            "configure_jurisdiction",
            json!({"town": "Worchester"}),
            "Worcester",
        ),
        (
            "read_local_file",
            json!({"path": "../../etc/passwd"}),
            "'..'",
        ),
        (
            "read_local_file",
            json!({"path": "/etc/passwd"}),
            "outside the data folder",
        ),
        (
            "read_local_file",
            json!({"path": "missing.csv"}),
            "not found",
        ),
        (
            "calculate_rates",
            json!({"groups": [{"label": "x", "count": 1, "population": 0}]}),
            "positive",
        ),
        (
            "lookup_reportable_condition",
            json!({"query": "  "}),
            "empty",
        ),
        (
            "check_document_accessibility",
            json!({"path": "line_list.csv"}),
            "DOCX, HTML, and PDF",
        ),
        (
            "apply_small_cell_suppression",
            json!({"path": "line_list.csv"}),
            "record-level",
        ),
        (
            "export_document",
            json!({"title": "x", "content_markdown": "![](chart.png)"}),
            "alt text",
        ),
    ];
    for (name, args, expect) in cases {
        let r = h.call(name, args.clone()).await;
        assert_eq!(r.is_error, Some(true), "{name} {args} should fail");
        let msg = everything(&r);
        assert!(
            msg.contains(expect),
            "{name}: '{msg}' should mention '{expect}'"
        );
    }
}

#[tokio::test]
async fn toolsets_limit_which_tools_are_listed_and_callable() {
    let h = start(|c| c.env_toolsets = Some([phn_core::Toolset::Cd].into())).await;
    let names: BTreeSet<String> = h
        .client
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();
    assert!(names.contains("lookup_reportable_condition"));
    assert!(names.contains("read_local_file")); // core is always on
    assert!(!names.contains("calculate_rates"));
    let r = h
        .call(
            "calculate_rates",
            json!({"groups": [{"label": "a", "count": 1, "population": 10}]}),
        )
        .await;
    assert_eq!(r.is_error, Some(true));
    assert!(everything(&r).contains("not enabled"));
}

#[tokio::test]
async fn configuring_toolsets_refreshes_the_tool_list() {
    let h = start(|_| {}).await;
    assert_eq!(
        h.client.list_all_tools().await.unwrap().len(),
        all_tools().len()
    );
    let r = h
        .call(
            "configure_jurisdiction",
            json!({"profile": "school_nurse", "toolsets": ["cd"]}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", summary(&r));
    assert_eq!(
        r.structured_content.unwrap()["data"]["toolsets_changed"],
        true
    );
    let names: Vec<String> = h
        .client
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();
    assert!(!names.contains(&"calculate_rates".to_string()));
    // Settings persist to /cache.
    assert!(h.dir.path().join("cache/phn-settings.toml").exists());
}

#[tokio::test]
async fn jurisdiction_flows_into_reportable_lookup() {
    let h = start(|_| {}).await;
    let r = h
        .call(
            "configure_jurisdiction",
            json!({"town": "amherst", "board_of_health_phone": "413-259-3077"}),
        )
        .await;
    let data = r.structured_content.unwrap()["data"].clone();
    assert_eq!(data["settings"]["jurisdiction"]["county"], "Hampshire");
    assert_eq!(data["settings"]["jurisdiction"]["hmcc_region"], "1");
    // The Phase 0 draft listed pertussis as immediately reportable by telephone.
    // The 2026-11-07 re-check found that DPH's immediate-call list (provider list,
    // June 2026; memo, May 20, 2026) does not include pertussis; it falls under
    // 105 CMR 300.100's "immediately, but in no case more than 24 hours". The test
    // still checks the alias match and the timeframe, now against the corrected content.
    let r = h
        .call(
            "lookup_reportable_condition",
            json!({"query": "whooping cough"}),
        )
        .await;
    let md = summary(&r);
    assert!(md.contains("Pertussis") && md.contains("within 24 hours"));
    assert!(md.contains("105 CMR 300.100"));
    assert!(md.contains("Your board of health: Amherst, 413-259-3077"));
    let s = r.structured_content.clone().unwrap();
    assert_eq!(s["data"]["condition"]["timeframe"], "within_24_hours");
    // An immediately reportable condition still says so.
    let m = h
        .call("lookup_reportable_condition", json!({"query": "measles"}))
        .await;
    assert!(summary(&m).contains("IMMEDIATELY"));
    assert_eq!(
        m.structured_content.unwrap()["data"]["condition"]["timeframe"],
        "immediate"
    );
    assert_eq!(
        s["notice"]
            .as_str()
            .map(|n| n.starts_with("Decision support")),
        Some(true)
    );
    let r = h
        .call("lookup_reportable_condition", json!({"query": "head lice"}))
        .await;
    let s = r.structured_content.clone().unwrap();
    assert_eq!(s["data"]["matched"], false);
    assert!(summary(&r).contains("does not mean it is not reportable"));
}

#[tokio::test]
async fn rates_match_the_exact_poisson_interval() {
    let h = start(|_| {}).await;
    let r = h
        .call(
            "calculate_rates",
            json!({"groups": [{"label": "Town", "count": 5, "population": 100000}], "per": 100000}),
        )
        .await;
    let rate = &r.structured_content.unwrap()["data"]["rates"][0];
    assert!((rate["lower"].as_f64().unwrap() - 1.6235).abs() < 1e-3);
    assert!((rate["upper"].as_f64().unwrap() - 11.6683).abs() < 1e-3);
    assert!(
        rate["flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "unreliable")
    );
}

// ---------------------------------------------------------------------------
// Privacy guardrails (Phase 1 exit criteria)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn phi_detector_flags_every_seeded_identifier() {
    let data = common::corpus().join("data");
    // Tables: every seeded column is flagged.
    for file in [
        "line_list.csv",
        "exclusion_letters.csv",
        "church_supper_outbreak.csv",
    ] {
        let t = phn_local::table::read_table(&data.join(file), None).unwrap();
        let flagged: BTreeSet<String> = phn_local::phi::scan_table(&t)
            .into_iter()
            .map(|f| f.column)
            .collect();
        for s in common::seeded()
            .iter()
            .filter(|s| s.file == format!("data/{file}"))
        {
            let col = s.column.as_ref().unwrap();
            assert!(
                flagged.contains(col),
                "{file}: column '{col}' ({}) not flagged; flagged {flagged:?}",
                s.kind
            );
        }
    }
    // Free text: every seeded value is covered by a strict redaction.
    let note = std::fs::read_to_string(data.join("client_note.txt")).unwrap();
    let red = phn_local::phi::redact_text(&note, phn_local::phi::RedactionLevel::Strict);
    let missed = leaks(&red.text, "data/client_note.txt", None);
    assert!(missed.is_empty(), "not redacted: {missed:?}\n{}", red.text);
    let policy = std::fs::read_to_string(data.join("policies/needlestick_exposure.md")).unwrap();
    let red = phn_local::phi::redact_text(&policy, phn_local::phi::RedactionLevel::Standard);
    assert!(leaks(&red.text, "data/policies/needlestick_exposure.md", None).is_empty());
}

#[tokio::test]
async fn no_row_level_data_is_returned_by_default() {
    let h = start(|_| {}).await;
    let ll = "data/line_list.csv";
    let checks = vec![
        ("read_local_file", json!({}), ll),
        ("read_local_file", json!({"path": "line_list.csv"}), ll),
        (
            "read_local_file",
            json!({"path": "/data/line_list.csv", "action": "read"}),
            ll,
        ),
        (
            "profile_local_dataset",
            json!({"path": "line_list.csv"}),
            ll,
        ),
        ("deidentify_dataset", json!({"path": "line_list.csv"}), ll),
        (
            "deidentify_dataset",
            json!({"path": "line_list.csv", "mode": "limited_data_set"}),
            ll,
        ),
        (
            "apply_small_cell_suppression",
            json!({"path": "line_list.csv"}),
            ll,
        ),
        (
            "read_local_file",
            json!({"path": "client_note.txt"}),
            "data/client_note.txt",
        ),
        (
            "search_agency_policies",
            json!({"query": "SSN form field example"}),
            "data/policies/needlestick_exposure.md",
        ),
        (
            "read_local_file",
            json!({"path": "policies/needlestick_exposure.md"}),
            "data/policies/needlestick_exposure.md",
        ),
        (
            "export_document",
            json!({
                "title": "Exclusion notice for {{Student}}",
                "content_markdown": "Dear {{Parent Name}},\n\n{{Student}} may return to {{School}} on {{Return Date}}.",
                "merge_data_path": "exclusion_letters.csv",
                "formats": ["docx", "pdf"]
            }),
            "data/exclusion_letters.csv",
        ),
        (
            "export_document",
            json!({
                "title": "Letter",
                "content_markdown": "Dear {{Parent Name}}, about {{Student}}.",
                "merge_data_path": "exclusion_letters.csv",
                "merge_mode": "one_file_per_record"
            }),
            "data/exclusion_letters.csv",
        ),
        // ---- Phase 2 data tools (EPI-10 run_statistical_test, EBP-06 create_spc_chart, ...)
        (
            "run_statistical_test",
            json!({"path": "line_list.csv", "outcome": "Age", "group": "Diagnosis"}),
            ll,
        ),
        (
            "run_statistical_test",
            json!({"path": "line_list.csv", "outcome": "Hospitalized", "group": "Sex"}),
            ll,
        ),
        (
            "run_statistical_test",
            json!({"path": "line_list.csv", "outcome": "Age", "group": "Last Name"}),
            ll,
        ),
        (
            "run_statistical_test",
            json!({"path": "line_list.csv", "outcome": "Age", "group": "Street Address"}),
            ll,
        ),
        (
            "create_spc_chart",
            json!({"path": "line_list.csv", "date_column": "Onset Date", "period": "week"}),
            ll,
        ),
        (
            "create_spc_chart",
            json!({"path": "line_list.csv", "date_column": "Onset Date", "measure": "mean", "value_column": "Age"}),
            ll,
        ),
        // ---- Phase 3 data tools (CD-04 analyze_outbreak_line_list, CD-05, CD-06, ...)
        (
            "analyze_outbreak_line_list",
            json!({"path": "line_list.csv"}),
            ll,
        ),
        (
            "analyze_outbreak_line_list",
            json!({"path": "line_list.csv", "disease": "pertussis", "town_column": "Town"}),
            ll,
        ),
        (
            "analyze_outbreak_line_list",
            json!({"path": "line_list.csv", "town_column": "Street Address"}),
            ll,
        ),
        (
            "analyze_outbreak_line_list",
            json!({"path": "line_list.csv", "case_column": "Last Name"}),
            ll,
        ),
        (
            "analyze_outbreak_line_list",
            json!({"path": "church_supper_outbreak.csv", "disease": "salmonellosis"}),
            "data/church_supper_outbreak.csv",
        ),
        (
            "analyze_outbreak_line_list",
            json!({"path": "church_supper_outbreak.csv", "exposure_columns": ["First Name"]}),
            "data/church_supper_outbreak.csv",
        ),
        (
            "generate_epi_curve",
            json!({"path": "line_list.csv", "stratify_by": "Diagnosis"}),
            ll,
        ),
        (
            "generate_epi_curve",
            json!({"path": "line_list.csv", "stratify_by": "Last Name"}),
            ll,
        ),
        (
            "generate_epi_curve",
            json!({"path": "church_supper_outbreak.csv", "disease": "salmonellosis",
                   "stratify_by": "Town", "exposure_datetime": "2026-05-02 18:00"}),
            "data/church_supper_outbreak.csv",
        ),
        (
            "generate_epi_curve",
            json!({"path": "church_supper_outbreak.csv", "stratify_by": "Phone"}),
            "data/church_supper_outbreak.csv",
        ),
        // ---- Phase 4 data tools (EQ-04 measure_health_disparities, EPI-12, ...)
        (
            "measure_health_disparities",
            json!({"path": "line_list.csv"}),
            ll,
        ),
        (
            "measure_health_disparities",
            json!({"path": "line_list.csv", "group_column": "Diagnosis", "count_column": "Age", "population_column": "Age"}),
            ll,
        ),
        (
            "measure_health_disparities",
            json!({"path": "line_list.csv", "group_column": "First Name", "count_column": "Age", "population_column": "Age"}),
            ll,
        ),
        (
            "create_choropleth_map",
            json!({"path": "line_list.csv", "geography": "town", "title": "Cases"}),
            ll,
        ),
        (
            "create_choropleth_map",
            json!({"path": "line_list.csv", "geography": "town", "title": "Cases", "area_column": "Last Name", "value_column": "Age"}),
            ll,
        ),
        (
            "create_choropleth_map",
            json!({"path": "line_list.csv", "geography": "zcta", "title": "Cases", "area_column": "ZIP", "value_column": "Age", "value_is_count": true}),
            ll,
        ),
        // ---- Phase 5 data tools (VAX-05 analyze_vaccination_coverage, ...)
        (
            "check_ma_school_immunization_requirements",
            json!({"roster_path": "school_roster.csv", "as_of": "2026-09-02"}),
            "data/school_roster.csv",
        ),
        (
            "check_ma_school_immunization_requirements",
            json!({"roster_path": "line_list.csv", "as_of": "2026-09-02"}),
            ll,
        ),
        (
            "analyze_vaccination_coverage",
            json!({"statewide": true, "local_path": "school_roster.csv"}),
            "data/school_roster.csv",
        ),
        (
            "analyze_vaccination_coverage",
            json!({"statewide": true, "local_path": "line_list.csv"}),
            ll,
        ),
        // ---- Phase 6 data tools (VAX-03 forecast_immunizations_due from a local file, ...)
        (
            "forecast_immunizations_due",
            json!({"file": "immunization_history.csv"}),
            "data/immunization_history.csv",
        ),
        (
            "forecast_immunizations_due",
            json!({"file": "immunization_history.csv", "vaccine_groups": ["MMR"], "assessment_date": "2026-11-07"}),
            "data/immunization_history.csv",
        ),
        // Pointed at the outbreak line list (wrong columns): the tool error
        // must not echo identifiers either.
        (
            "forecast_immunizations_due",
            json!({"file": "line_list.csv", "id_column": "Case ID", "dob_column": "DOB", "date_column": "Onset Date", "vaccine_column": "Diagnosis"}),
            ll,
        ),
        // ---- Phase 7 data tools go below this line.
        (
            "track_school_screenings",
            json!({"path": "school_screenings.csv", "as_of": "2026-11-15", "write_letters": true}),
            "data/school_screenings.csv",
        ),
        (
            "track_school_screenings",
            json!({"path": "line_list.csv"}),
            ll,
        ),
        (
            "monitor_school_absenteeism",
            json!({"path": "school_absence_log.csv"}),
            "data/school_absence_log.csv",
        ),
        (
            "monitor_school_absenteeism",
            json!({"path": "school_absence_log.csv", "group_column": "Student Name"}),
            "data/school_absence_log.csv",
        ),
        (
            "monitor_school_absenteeism",
            json!({"path": "line_list.csv", "school_column": "Town", "reason_column": "Diagnosis", "date_column": "Onset Date"}),
            ll,
        ),
        (
            "build_school_health_annual_report",
            json!({"visit_log_path": "health_office_visits.csv", "screening_path": "school_screenings.csv", "school_year": "2025-2026"}),
            "data/health_office_visits.csv",
        ),
        (
            "build_school_health_annual_report",
            json!({"screening_path": "school_screenings.csv", "school_year": "2026-2027"}),
            "data/school_screenings.csv",
        ),
        (
            "concussion_return_to_learn_tracker",
            json!({"tracker_path": "concussion_log.csv", "write_plans": true, "as_of": "2026-11-01"}),
            "data/concussion_log.csv",
        ),
        (
            "build_individualized_health_plan",
            json!({"condition": "food allergy", "student_file": "health_plan_students.csv"}),
            "data/health_plan_students.csv",
        ),
        (
            "build_individualized_health_plan",
            json!({"condition": "diabetes", "student_file": "health_plan_students.csv"}),
            "data/health_plan_students.csv",
        ),
        // ---- End of Phase 7 data tools.
        // ---- Phase 8 data tools go below this line.
        (
            "calculate_growth_percentiles",
            json!({"measurements_path": "growth_measurements.csv"}),
            "data/growth_measurements.csv",
        ),
        // Pointed at the outbreak line list (no growth columns): the error must not leak.
        (
            "calculate_growth_percentiles",
            json!({"measurements_path": "line_list.csv"}),
            ll,
        ),
        (
            "mandated_reporter_51a_guide",
            json!({"notes_path": "51a_notes.txt", "narrative_template": true}),
            "data/51a_notes.txt",
        ),
        (
            "mandated_reporter_51a_guide",
            json!({"notes_path": "line_list.csv"}),
            ll,
        ),
        // ---- End of Phase 8 data tools.
        // ---- Phase 9 data tools go below this line.
        (
            "plan_tb_contact_investigation",
            json!({"index_case": {"symptomatic": true, "afb_smear_positive": true, "symptom_onset_date": "2026-06-01"},
                   "contact_file": "tb_contacts.csv", "listed_date": "2026-08-07"}),
            "data/tb_contacts.csv",
        ),
        (
            "plan_tb_contact_investigation",
            json!({"index_case": {"symptomatic": true, "afb_smear_positive": true, "symptom_onset_date": "2026-06-01"},
                   "contact_file": "line_list.csv"}),
            ll,
        ),
        (
            "screen_health_related_social_needs",
            json!({"path": "hrsn_screenings.csv", "group_by": "Site"}),
            "data/hrsn_screenings.csv",
        ),
        (
            "screen_health_related_social_needs",
            json!({"path": "hrsn_screenings.csv", "group_by": "Client Name"}),
            "data/hrsn_screenings.csv",
        ),
        (
            "screen_health_related_social_needs",
            json!({"path": "line_list.csv"}),
            ll,
        ),
        (
            "omaha_system_coder",
            json!({"path": "client_note.txt", "write_care_plan": true}),
            "data/client_note.txt",
        ),
        (
            "nursing_documentation_template",
            json!({"check_path": "client_note.txt", "visit_type": "home_visit"}),
            "data/client_note.txt",
        ),
        (
            "classify_intervention_wheel",
            json!({"path": "line_list.csv", "activity_column": "Notes", "group_by": "Diagnosis"}),
            ll,
        ),
        (
            "classify_intervention_wheel",
            json!({"path": "line_list.csv", "activity_column": "Notes", "group_by": "Last Name"}),
            ll,
        ),
        (
            "find_community_resources",
            json!({"directory_path": "line_list.csv", "town": "Amherst"}),
            ll,
        ),
        (
            "find_community_resources",
            json!({"directory_path": "tb_contacts.csv", "town": "Amherst"}),
            "data/tb_contacts.csv",
        ),
        // ---- End of Phase 9 data tools.
        // ---- Phase 10 data tools go below this line.
        (
            "score_mental_health_screen",
            json!({"instrument": "phq9", "file": "phq9_screens.csv"}),
            "data/phq9_screens.csv",
        ),
        (
            "score_mental_health_screen",
            json!({"instrument": "phq9", "file": "line_list.csv"}),
            ll,
        ),
        (
            "elder_protective_services_guide",
            json!({"age_years": 80, "details_file": "protective_report_details.csv"}),
            "data/protective_report_details.csv",
        ),
        (
            "elder_protective_services_guide",
            json!({"details_file": "line_list.csv", "format": "pdf"}),
            ll,
        ),
        (
            "advance_care_planning_toolkit",
            json!({"details_file": "proxy_details.csv"}),
            "data/proxy_details.csv",
        ),
        (
            "advance_care_planning_toolkit",
            json!({"details_file": "line_list.csv"}),
            ll,
        ),
        // ---- End of Phase 10 data tools.
        // ---- Phase 11 data tools go below this line.
        (
            "check_readability",
            json!({"path": "client_note.txt"}),
            "data/client_note.txt",
        ),
        (
            "check_readability",
            json!({"path": "policies/needlestick_exposure.md", "max_flags": 50}),
            "data/policies/needlestick_exposure.md",
        ),
        (
            "plan_school_postvention",
            json!({"details_path": "postvention_details.csv", "cause_status": "ruled_suicide",
                   "messages": ["staff_meeting", "student_notification", "parent_letter", "media_statement", "social_media_post"]}),
            "data/postvention_details.csv",
        ),
        (
            "plan_school_postvention",
            json!({"draft_path": "client_note.txt"}),
            "data/client_note.txt",
        ),
        (
            "translate_health_material",
            json!({"source_path": "client_note.txt", "language": "Spanish", "worksheet": "xlsx"}),
            "data/client_note.txt",
        ),
        // COMM-03 pointed at the outbreak line list as its observance calendar.
        (
            "create_social_media_campaign",
            json!({"topic": "flu vaccine", "platforms": ["facebook"], "observances_path": "line_list.csv"}),
            ll,
        ),
        // ---- End of Phase 11 data tools.
        // ---- Phase 12 data tools go below this line.
        (
            "functional_needs_shelter_intake",
            json!({"area": "Brockton", "expected_residents": 150, "intake_path": "shelter_registrations.csv"}),
            "data/shelter_registrations.csv",
        ),
        (
            "heat_health_action_planner",
            json!({"area": "Springfield", "plan_for_level": 3, "cooling_centers_path": "cooling_centers.csv"}),
            "data/cooling_centers.csv",
        ),
        (
            "plan_casper_assessment",
            json!({"area": "Fall River", "seed": 7, "results_path": "casper_results.csv",
                   "times_selected_column": "times_selected",
                   "questions": ["structure", "damage", "has_generator", "needs_medication"]}),
            "data/casper_results.csv",
        ),
        // Identifier columns named as questions are refused without echoing values.
        (
            "plan_casper_assessment",
            json!({"area": "Fall River", "seed": 7, "results_path": "casper_results.csv",
                   "questions": ["Household Address", "Callback Phone"]}),
            "data/casper_results.csv",
        ),
        // Individual-level files given to aggregate-only readers are refused.
        (
            "get_electricity_dependent_population",
            json!({"area": "Brockton", "local_path": "shelter_registrations.csv"}),
            "data/shelter_registrations.csv",
        ),
        (
            "hazard_vulnerability_assessment",
            json!({"area": "Plymouth County", "nri_path": "shelter_registrations.csv"}),
            "data/shelter_registrations.csv",
        ),
        (
            "plan_casper_assessment",
            json!({"area": "Brockton", "results_path": "shelter_registrations.csv", "cluster_column": "Name"}),
            "data/shelter_registrations.csv",
        ),
        // ---- End of Phase 12 data tools.
        // ---- Phase 13 EPR-01 (ICS) data tools go below this line.
        (
            "generate_ics_forms",
            json!({"forms": ["all"], "roster_path": "staff_roster.csv", "incident_name": "Leak test",
                   "blank_forms": true}),
            "data/staff_roster.csv",
        ),
        (
            "generate_ics_forms",
            json!({"roster_path": "staff_roster.csv",
                   "organization": [{"position": "Incident Commander"}, {"position": "Safety Officer"}],
                   "assignments": [{"group": "Clinic Group", "supervisor": "R. Example",
                                    "resources": [{"identifier": "Team 1", "leader": "R. Example"}]}]}),
            "data/staff_roster.csv",
        ),
        // A client file given as the roster is used only in the documents.
        (
            "generate_ics_forms",
            json!({"forms": ["203", "205A", "207"], "roster_path": "line_list.csv"}),
            ll,
        ),
        // ---- End of Phase 13 EPR-01 data tools.
        // ---- Phase 13 EPR-07 (COOP) data tools go below this line.
        // EPR-07 fills the staff roster's names and contacts only into files.
        (
            "continuity_of_operations_planner",
            json!({"town": "Wilmington", "roster_path": "staff_roster.csv", "output": "both",
                   "succession": [{"position": "Incident Commander",
                                   "successors": ["Health Agent", "Public Health Nurse", "Safety Officer"]}]}),
            "data/staff_roster.csv",
        ),
        // A client file given as a roster is used only inside the files, never echoed.
        (
            "continuity_of_operations_planner",
            json!({"roster_path": "shelter_registrations.csv", "output": "none"}),
            "data/shelter_registrations.csv",
        ),
        // ---- End of Phase 13 EPR-07 data tools.
        // ---- Phase 13 EPR-08/EPR-09 (HSEEP) data tools go below this line.
        // ---- End of Phase 13 EPR-08/EPR-09 data tools.
        // ---- Phase 13 EPR-10 (CERC) data tools go below this line.
        // ---- End of Phase 13 EPR-10 data tools.
    ];
    for (name, args, file) in checks {
        let r = h.call(name, args.clone()).await;
        let found = leaks(&everything(&r), file, None);
        assert!(
            found.is_empty(),
            "{name} {args} returned identifiers: {found:?}"
        );
    }
    // Aggregate value summaries never show direct identifiers either.
    let r = h
        .call(
            "profile_local_dataset",
            json!({"path": "line_list.csv", "include_value_summaries": true}),
        )
        .await;
    assert!(leaks(&everything(&r), ll, None).is_empty());
    let s = r.structured_content.unwrap();
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert_eq!(s["privacy"]["suppression_applied"], true);
}

#[tokio::test]
async fn requested_rows_are_masked() {
    let h = start(|_| {}).await;
    let r = h
        .call(
            "read_local_file",
            json!({"path": "line_list.csv", "include_rows": true, "max_rows": 25}),
        )
        .await;
    assert_eq!(r.is_error, Some(false));
    let all = everything(&r);
    let found = leaks(&all, "data/line_list.csv", None);
    assert!(found.is_empty(), "masked rows leaked: {found:?}");
    let s = r.structured_content.unwrap();
    assert_eq!(s["privacy"]["row_level_data"], true);
    let rows = s["data"]["tables"][0]["sample_rows"].as_array().unwrap();
    assert_eq!(rows.len(), 25);
    assert!(all.contains("90+"), "ages over 89 are top-coded");
    assert!(all.contains("[REDACTED: names]"));
}

#[tokio::test]
async fn deidentified_file_on_disk_is_clean() {
    let h = start(|_| {}).await;
    let r = h
        .call(
            "deidentify_dataset",
            json!({"path": "line_list.csv", "reference_date": "2026-11-01"}),
        )
        .await;
    let s = r.structured_content.unwrap();
    assert_eq!(s["data"]["safe_harbor_compliant"], true);
    let out = h.data().join(s["data"]["output_file"].as_str().unwrap());
    let text = std::fs::read_to_string(&out).unwrap();
    let found = leaks(&text, "data/line_list.csv", None);
    assert!(found.is_empty(), "de-identified file contains {found:?}");
    for town in ["Amherst", "Hadley", "Pelham", "Northampton"] {
        assert!(
            !text.contains(town),
            "town {town} must be removed under Safe Harbor"
        );
    }
    assert!(text.contains("90+") && text.contains("010"));
    let report = h.data().join(s["data"]["report_file"].as_str().unwrap());
    assert!(
        std::fs::read_to_string(report)
            .unwrap()
            .contains("Safe Harbor")
    );
}

#[tokio::test]
async fn suppression_hides_small_cells_and_their_complements() {
    let h = start(|_| {}).await;
    let r = h
        .call(
            "apply_small_cell_suppression",
            json!({"path": "counts_by_town.csv"}),
        )
        .await;
    let d = r.structured_content.unwrap()["data"].clone();
    let rows = d["rows"].as_array().unwrap();
    // Amherst 2025 = 3 and Hadley 2024 = 2 are primary; Pelham's 1s are too.
    assert_eq!(rows[0][2], "*");
    assert_eq!(rows[1][1], "*");
    assert!(d["complementary_suppressed"].as_u64().unwrap() >= 1);
    // No count from 1 to 4 survives.
    for row in rows {
        for cell in row.as_array().unwrap().iter().skip(1) {
            if let Ok(n) = cell.as_str().unwrap().parse::<u64>() {
                assert!(n == 0 || n >= 5, "small count {n} shown");
            }
        }
    }
    assert!(h.data().join(d["output_file"].as_str().unwrap()).exists());
}

#[tokio::test]
async fn exported_documents_pass_the_servers_own_accessibility_check() {
    let h = start(|_| {}).await;
    h.call("configure_jurisdiction", json!({"letterhead": {"agency_name": "Example Health Department", "phone": "413-555-0100", "address_lines": ["1 Main St", "Example, MA 01000"]}})).await;
    let r = h.call("export_document", json!({
        "title": "Pertussis in schools",
        "subtitle": "Guidance for families",
        "content_markdown": "# What is pertussis?\n\nA **contagious** cough illness. See [DPH guidance](https://www.mass.gov).\n\n## What to do\n\n1. Keep sick children home.\n2. Call your doctor.\n\nTable: Cases by week\n\n| Week | Cases |\n|---|---|\n| 1 | 2 |\n| 2 | 6 |\n",
        "formats": ["docx", "pdf"],
        "language": "en-US"
    })).await;
    assert_eq!(r.is_error, Some(false), "{}", summary(&r));
    let files = r.structured_content.unwrap()["data"]["files"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(files.len(), 2);
    for f in files {
        let path = f["path"].as_str().unwrap();
        let r = h
            .call("check_document_accessibility", json!({"path": path}))
            .await;
        assert_eq!(r.is_error, Some(false), "{}", summary(&r));
        let report = &r.structured_content.unwrap()["data"]["report"];
        assert_eq!(report["failed"], 0, "{path}: {}", report);
        assert_eq!(report["warnings"], 0, "{path}: {}", report);
    }
}

#[tokio::test]
async fn policy_search_cites_files_and_sections() {
    let h = start(|_| {}).await;
    // Add a DOCX policy so DOCX sections are indexed too.
    let doc = phn_docs_fixture();
    std::fs::write(h.data().join("policies/communicable_disease.docx"), doc).unwrap();
    let r = h
        .call(
            "search_agency_policies",
            json!({"query": "measles exclusion"}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", summary(&r));
    let hits = r.structured_content.unwrap()["data"]["hits"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        hits[0]["citation"],
        "communicable_disease.docx, § Measles exposure in schools"
    );
    let r = h
        .call("search_agency_policies", json!({"query": "needlestick"}))
        .await;
    let hits = r.structured_content.unwrap()["data"]["hits"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(hits[0]["file"], "needlestick_exposure.md");
    assert!(
        hits[0]["citation"]
            .as_str()
            .unwrap()
            .contains("Needlestick and sharps injuries")
    );
}

fn phn_docs_fixture() -> Vec<u8> {
    // Built with the server's own DOCX writer to keep the fixture readable.
    let md = "# Measles exposure in schools\n\nSusceptible students are excluded from day 5 through day 21 after exposure.\n\n# Influenza\n\nEncourage annual vaccination.";
    let doc = phn_docs::model::Document {
        title: "Communicable disease procedures".into(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: None,
        logo: None,
        blocks: phn_docs::model::from_markdown(md),
    };
    phn_docs::docx::to_docx(&doc).unwrap()
}

#[tokio::test]
async fn resources_expose_content_and_snapshots() {
    let h = start(|_| {}).await;
    let resources = h.client.list_all_resources().await.unwrap();
    let uris: Vec<String> = resources.iter().map(|r| r.uri.clone()).collect();
    for u in [
        "phn://content/ma-reportable-conditions",
        "phn://snapshots/ma_municipalities.csv",
        "phn://snapshots/manifest.json",
    ] {
        assert!(uris.contains(&u.to_string()), "{u} missing from {uris:?}");
    }
    let r = h
        .client
        .read_resource(rmcp::model::ReadResourceRequestParams::new(
            "phn://snapshots/ma_municipalities.csv",
        ))
        .await
        .unwrap();
    let text = match &r.contents[0] {
        rmcp::model::ResourceContents::TextResourceContents { text, .. } => text.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(text.lines().count(), 352);
}

#[tokio::test]
async fn data_source_status_reports_offline_mode_and_review_state() {
    let h = start(|_| {}).await;
    let r = h
        .call("get_data_source_status", json!({"check_live": true}))
        .await;
    let d = r.structured_content.unwrap()["data"].clone();
    assert_eq!(d["offline_mode"], true);
    let sources = d["sources"].as_array().unwrap();
    let rc = sources
        .iter()
        .find(|s| s["id"] == "ma-reportable-conditions")
        .unwrap();
    assert_eq!(rc["review_status"], "pending review");
    let towns = sources
        .iter()
        .find(|s| s["id"] == "ma-municipalities")
        .unwrap();
    assert_eq!(towns["snapshot_origin"], "bundled");
    assert_eq!(towns["live_check"]["reachable"], false);
    assert!(sources.iter().all(|s| s["status"] != "planned"));
}
