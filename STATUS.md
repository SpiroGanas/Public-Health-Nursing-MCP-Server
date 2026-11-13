# Project status and handoff

A handoff document for whoever picks up this project next, human or AI agent. Read it before
starting work, and **update it before you finish** (see [Keeping this file current](#keeping-this-file-current)).

- **Last updated:** 2026-11-13
- **Current version:** 1.0.0 (all of Phases 0–19 of [`project_plan.md`](project_plan.md), 148 of
  148 tools); not yet released. Phases 15–19 were built by fourteen helpers and merged on branch
  `phase-19-docs` (PR: this branch; the number is not known yet). The workspace version in
  `Cargo.toml` still reads 0.15.0: bump it to 1.0.0 (and regenerate `Cargo.lock`) before tagging.
- **Next:** none. The plan is complete; what remains is post-1.0 maintenance
  (`project_plan.md` section 8) and the open items in section 2. No phase has yet met the plan's
  definition of done (content reviewed, signed image published, acceptance script run). All
  bundled content is `status: pending` with review due 2027-02-01, and **CI starts failing after
  that date** until each `meta.review` is updated.

## 1. Where things stand

| Phase | Theme | State | Where |
|---|---|---|---|
| — | Tool brainstorm and project plan | Merged | PR #1 |
| 0–1 | Foundation; local data, privacy guardrails, documents (11 tools) | Merged; awaiting acceptance | PR #2 |
| 2 | Epidemiology and biostatistics engine (8 tools) | Merged; awaiting acceptance | PR #3 |
| 3 | Communicable disease I (8 tools + outbreak prompt) | Merged; content review needed before Pilot 1 | PR #3 |
| 4 | Public data and evidence platform (8 tools) | Merged; awaiting acceptance | PR #3 |
| 5 | Immunizations I (9 tools) | Merged; content review needed | PR #3 |
| 6 | Immunization forecasting and surveillance feeds (3 tools) | Merged; CDSi exceptions to review | PR #3 |
| — | User guide for nurses (Word) | Merged; nurse to read for accuracy and tone | PR #4 |
| 7 | School health (8 tools) | Merged; content review needed before Pilot 2 | PR #5 |
| 8 | Maternal, child, and family health (8 tools) | Merged; content review needed | PR #5 |
| 9 | PHN practice toolkit and case management (8 tools) | Merged; content review needed | PR #5 |
| 10 | Healthy aging and mental health screening (9 tools) | Merged; content review needed | PR #5 |
| 11 | Substance use, crisis resources, and health communication (9 tools) | Merged; content review needed | PR #5 |
| 12 | Emergency preparedness I (8 tools) | Merged; content review needed | PR #5 |
| 13 | Emergency preparedness II: ICS, plans, and exercises (5 tools + tabletop prompt) | Merged; content review needed before Pilot 3 | PR #6 |
| — | Peer review of Phases 0–13: code, plan, architecture, and documentation | Merged | PR #7 |
| 14 | Environmental health and climate (8 tools) | Merged; content review needed | PR #8 |
| 15 | Chronic disease prevention and community assessment (8 tools + community health assessment prompt) | Merged to this branch; content review needed | PR: this branch |
| 16 | Program planning, evaluation, and evidence (9 tools) | Merged to this branch; content review needed | PR: this branch |
| 17 | Grants and finance (7 tools + grant application prompt) | Merged to this branch; content review needed; federal grant search never run live | PR: this branch |
| 18 | Leadership, accreditation, and professional practice (9 tools) | Merged to this branch; content review needed | PR: this branch |
| 19 | Policy and law (5 tools); v1.0 hardening | Merged to this branch; content review needed; Federal Register and Regulations.gov adapters never run live | PR: this branch |

PRs #5 (Phases 7–12), #6 (Phase 13), #7 (the peer review: about 170 of roughly 210 findings
fixed; the rest are open items in section 2), and #8 (Phase 14) are merged. Phases 15–19 were
built the same way: fourteen helpers each built one work package in a separate worktree
(screen, prevent, cha, geo; design, interventions, appraise, qi; grants, budget, finance;
strategy, workforce, ethics; tracking, law) and wrote a notes file for the coordinator, and the
coordinator merged them and folded the notes into `CHANGELOG.md`, `docs/acceptance/`, this file,
and `docs/content-review-sweep.md`. Everything is on branch `phase-19-docs`; when it merges,
start new work on a fresh branch from `main`.

### Tools implemented (148 of 148)

| Tool | Plan ID | Toolset | Phase | Code |
|---|---|---|---|---|
| `get_data_source_status` | CORE-08 | core | 0 | `crates/phn-tools/src/core/status.rs` |
| `configure_jurisdiction` | CORE-09 | core | 0 | `crates/phn-tools/src/core/configure.rs` |
| `calculate_rates` | EPI-01 | epi | 0 | `crates/phn-tools/src/epi/rates.rs` |
| `lookup_reportable_condition` | CD-01 | cd | 0 | `crates/phn-tools/src/cd/reportable.rs` |
| `read_local_file` | CORE-04 | core | 1 | `crates/phn-tools/src/core/files.rs` |
| `search_agency_policies` | CORE-05 | core | 1 | `crates/phn-tools/src/core/policies.rs` |
| `export_document` | CORE-06 | core | 1 | `crates/phn-tools/src/core/export.rs` |
| `apply_small_cell_suppression` | EPI-06 | epi | 1 | `crates/phn-tools/src/epi/suppression.rs` |
| `profile_local_dataset` | EPI-07 | epi | 1 | `crates/phn-tools/src/epi/profile.rs` |
| `deidentify_dataset` | EPI-08 | epi | 1 | `crates/phn-tools/src/epi/deidentify.rs` |
| `check_document_accessibility` | COMM-05 | comms | 1 | `crates/phn-tools/src/comms/accessibility.rs` |
| `age_adjust_rates` | EPI-02 | epi | 2 | `crates/phn-tools/src/epi/age_adjust.rs` |
| `analyze_two_by_two` | EPI-03 | epi | 2 | `crates/phn-tools/src/epi/two_by_two.rs` |
| `calculate_sample_size` | EPI-04 | epi | 2 | `crates/phn-tools/src/epi/sample_size.rs` |
| `compute_standardized_ratio` | EPI-05 | epi | 2 | `crates/phn-tools/src/epi/standardized_ratio.rs` |
| `analyze_trends` | EPI-09 | epi | 2 | `crates/phn-tools/src/epi/trends.rs` |
| `run_statistical_test` | EPI-10 | epi | 2 | `crates/phn-tools/src/epi/stat_test.rs` |
| `create_chart` | EPI-11 | epi | 2 | `crates/phn-tools/src/epi/chart.rs` |
| `create_spc_chart` | EBP-06 | ebp | 2 | `crates/phn-tools/src/ebp/spc.rs` |
| `search_ma_disease_control_guide` | CD-02 | cd | 3 | `crates/phn-tools/src/cd/guide.rs` |
| `calculate_isolation_and_exclusion` | CD-03 | cd | 3 | `crates/phn-tools/src/cd/isolation.rs` |
| `analyze_outbreak_line_list` | CD-04 | cd | 3 | `crates/phn-tools/src/cd/outbreak.rs` |
| `generate_epi_curve` | CD-05 | cd | 3 | `crates/phn-tools/src/cd/epicurve.rs` |
| `plan_contact_tracing` | CD-06 | cd | 3 | `crates/phn-tools/src/cd/contact_tracing.rs` |
| `assess_measles_exposure` | CD-07 | cd | 3 | `crates/phn-tools/src/cd/measles.rs` |
| `detect_aberrations` | CD-10 | cd | 3 | `crates/phn-tools/src/cd/aberrations.rs` |
| `infection_control_for_congregate_settings` | CD-13 | cd | 3 | `crates/phn-tools/src/cd/infection_control.rs` |
| `build_community_health_profile` | EQ-01 | equity | 4 | `crates/phn-tools/src/equity/profile.rs` |
| `get_social_vulnerability_index` | EQ-02 | equity | 4 | `crates/phn-tools/src/equity/svi.rs` |
| `language_access_profile` | EQ-03 | equity | 4 | `crates/phn-tools/src/equity/language.rs` |
| `measure_health_disparities` | EQ-04 | equity | 4 | `crates/phn-tools/src/equity/disparities.rs` |
| `get_places_estimates` | CHR-01 | chronic | 4 | `crates/phn-tools/src/chronic/places.rs` |
| `create_choropleth_map` | EPI-12 | epi | 4 | `crates/phn-tools/src/epi/choropleth/mod.rs` |
| `search_pubmed` | EBP-01 | ebp | 4 | `crates/phn-tools/src/ebp/pubmed.rs` |
| `search_cdc_guidance` | EBP-02 | ebp | 4 | `crates/phn-tools/src/ebp/cdc_guidance.rs` |
| `get_immunization_schedule` | VAX-01 | vax | 5 | `crates/phn-tools/src/vax/schedule.rs` |
| `compare_vaccine_recommendations` | VAX-02 | vax | 5 | `crates/phn-tools/src/vax/compare.rs` |
| `check_ma_school_immunization_requirements` | VAX-04 | vax | 5 | `crates/phn-tools/src/vax/school.rs` |
| `analyze_vaccination_coverage` | VAX-05 | vax | 5 | `crates/phn-tools/src/vax/coverage.rs` |
| `vaccine_temperature_excursion_helper` | VAX-06 | vax | 5 | `crates/phn-tools/src/vax/excursion.rs` |
| `lookup_vaccine_product` | VAX-07 | vax | 5 | `crates/phn-tools/src/vax/products.rs` |
| `plan_vaccination_clinic` | VAX-08 | vax | 5 | `crates/phn-tools/src/vax/clinic.rs` |
| `vaccine_confidence_conversation_guide` | VAX-09 | vax | 5 | `crates/phn-tools/src/vax/confidence.rs` |
| `lookup_medication_information` | CORE-07 | core | 5 | `crates/phn-tools/src/core/medication.rs` |
| `forecast_immunizations_due` | VAX-03 | vax | 6 | `crates/phn-tools/src/vax/forecast.rs` |
| `get_notifiable_disease_trends` | CD-08 | cd | 6 | `crates/phn-tools/src/cd/nndss.rs` |
| `get_respiratory_and_wastewater_surveillance` | CD-09 | cd | 6 | `crates/phn-tools/src/cd/respiratory.rs` |
| `get_ma_school_screening_requirements` | SCH-01 | school | 7 | `crates/phn-tools/src/school/screening_requirements.rs` |
| `track_school_screenings` | SCH-02 | school | 7 | `crates/phn-tools/src/school/screenings.rs` |
| `school_medication_administration_guide` | SCH-03 | school | 7 | `crates/phn-tools/src/school/medication.rs` |
| `build_individualized_health_plan` | SCH-04 | school | 7 | `crates/phn-tools/src/school/health_plans.rs` |
| `concussion_return_to_learn_tracker` | SCH-05 | school | 7 | `crates/phn-tools/src/school/concussion.rs` |
| `monitor_school_absenteeism` | SCH-06 | school | 7 | `crates/phn-tools/src/school/absenteeism.rs` |
| `build_school_health_annual_report` | SCH-07 | school | 7 | `crates/phn-tools/src/school/annual_report.rs` |
| `child_care_health_consultation_checklist` | SCH-08 | school | 7 | `crates/phn-tools/src/school/child_care.rs` |
| `calculate_growth_percentiles` | MCH-03 | mch | 8 | `crates/phn-tools/src/mch/growth/mod.rs` |
| `childhood_lead_screening_guide` | MCH-04 | mch | 8 | `crates/phn-tools/src/mch/lead.rs` |
| `mandated_reporter_51a_guide` | MCH-07 | mch | 8 | `crates/phn-tools/src/mch/mandated_reporter.rs` |
| `check_developmental_milestones` | MCH-02 | mch | 8 | `crates/phn-tools/src/mch/milestones.rs` |
| `plan_newborn_postpartum_visit` | MCH-01 | mch | 8 | `crates/phn-tools/src/mch/newborn.rs` |
| `estimate_benefit_eligibility` | EQ-06 | equity | 8 | `crates/phn-tools/src/equity/benefits.rs` |
| `map_lead_risk` | MCH-05 | mch | 8 | `crates/phn-tools/src/mch/lead_risk.rs` |
| `maternal_infant_health_profile` | MCH-06 | mch | 8 | `crates/phn-tools/src/mch/maternal_infant.rs` |
| `plan_tb_contact_investigation` | CD-11 | cd | 9 | `crates/phn-tools/src/cd/tb_contacts.rs` |
| `ltbi_treatment_calendar` | CD-12 | cd | 9 | `crates/phn-tools/src/cd/ltbi_calendar.rs` |
| `screen_health_related_social_needs` | EQ-05 | equity | 9 | `crates/phn-tools/src/equity/hrsn.rs` |
| `omaha_system_coder` | CORE-01 | core | 9 | `crates/phn-tools/src/core/omaha.rs` |
| `classify_intervention_wheel` | CORE-02 | core | 9 | `crates/phn-tools/src/core/wheel.rs` |
| `nursing_documentation_template` | CORE-03 | core | 9 | `crates/phn-tools/src/core/documentation.rs` |
| `find_community_resources` | EQ-07 | equity | 9 | `crates/phn-tools/src/equity/resources.rs` |
| `newcomer_health_navigator` | EQ-11 | equity | 9 | `crates/phn-tools/src/equity/newcomer.rs` |
| `score_mental_health_screen` | MH-02 | bh | 10 | `crates/phn-tools/src/bh/screening.rs` |
| `find_crisis_resources` | MH-01 | bh | 10 | `crates/phn-tools/src/bh/crisis.rs` |
| `steadi_fall_risk_assessment` | AGE-01 | aging | 10 | `crates/phn-tools/src/aging/falls.rs` |
| `elder_protective_services_guide` | AGE-03 | aging | 10 | `crates/phn-tools/src/aging/protective.rs` |
| `find_ma_aging_services` | AGE-02 | aging | 10 | `crates/phn-tools/src/aging/services.rs` |
| `explain_older_adult_coverage_options` | AGE-04 | aging | 10 | `crates/phn-tools/src/aging/coverage.rs` |
| `nursing_home_quality_lookup` | AGE-06 | aging | 10 | `crates/phn-tools/src/aging/nursing_homes.rs` |
| `map_older_adult_vulnerability` | AGE-05 | aging | 10 | `crates/phn-tools/src/aging/vulnerability.rs` |
| `advance_care_planning_toolkit` | AGE-07 | aging | 10 | `crates/phn-tools/src/aging/acp.rs` |
| `check_readability` | COMM-01 | comms | 11 | `crates/phn-tools/src/comms/readability.rs` |
| `get_overdose_data` | MH-03 | bh | 11 | `crates/phn-tools/src/bh/overdose.rs` |
| `get_youth_risk_behavior_data` | MH-07 | bh | 11 | `crates/phn-tools/src/bh/youth_survey.rs` |
| `find_naloxone_and_harm_reduction` | MH-04 | bh | 11 | `crates/phn-tools/src/bh/harm_reduction.rs` |
| `find_substance_use_treatment` | MH-05 | bh | 11 | `crates/phn-tools/src/bh/treatment.rs` |
| `plan_school_postvention` | MH-06 | bh | 11 | `crates/phn-tools/src/bh/postvention.rs` |
| `translate_health_material` | COMM-02 | comms | 11 | `crates/phn-tools/src/comms/translation.rs` |
| `create_social_media_campaign` | COMM-03 | comms | 11 | `crates/phn-tools/src/comms/social.rs` |
| `draft_press_materials` | COMM-04 | comms | 11 | `crates/phn-tools/src/comms/press.rs` |
| `get_active_hazard_alerts` | EPR-05 | epr | 12 | `crates/phn-tools/src/epr/alerts.rs` |
| `model_dispensing_site_throughput` | EPR-02 | epr | 12 | `crates/phn-tools/src/epr/dispensing.rs` |
| `air_quality_protective_guidance` | ENV-02 | env | 12 | `crates/phn-tools/src/env/air_quality.rs` |
| `heat_health_action_planner` | ENV-01 | env | 12 | `crates/phn-tools/src/env/heat.rs` |
| `hazard_vulnerability_assessment` | EPR-06 | epr | 12 | `crates/phn-tools/src/epr/hva.rs` |
| `functional_needs_shelter_intake` | EPR-03 | epr | 12 | `crates/phn-tools/src/epr/shelter.rs` |
| `get_electricity_dependent_population` | EPR-04 | epr | 12 | `crates/phn-tools/src/epr/power.rs` |
| `plan_casper_assessment` | EPR-11 | epr | 12 | `crates/phn-tools/src/epr/casper.rs` |
| `generate_ics_forms` | EPR-01 | epr | 13 | `crates/phn-tools/src/epr/ics.rs` |
| `continuity_of_operations_planner` | EPR-07 | epr | 13 | `crates/phn-tools/src/epr/coop.rs` |
| `design_hseep_exercise` | EPR-08 | epr | 13 | `crates/phn-tools/src/epr/exercise.rs` |
| `build_after_action_report` | EPR-09 | epr | 13 | `crates/phn-tools/src/epr/aar.rs` |
| `crisis_risk_communication_builder` | EPR-10 | epr | 13 | `crates/phn-tools/src/epr/cerc.rs` |
| `drinking_water_quality_lookup` | ENV-03 | env | 14 | `crates/phn-tools/src/env/drinking_water.rs` |
| `community_water_fluoridation_status` | CHR-07 | chronic | 14 | `crates/phn-tools/src/chronic/fluoridation.rs` |
| `environmental_justice_screen` | ENV-04 | env | 14 | `crates/phn-tools/src/env/ej_screen.rs` |
| `climate_health_vulnerability_profile` | ENV-05 | env | 14 | `crates/phn-tools/src/env/climate_profile.rs` |
| `vector_borne_disease_risk` | ENV-06 | env | 14 | `crates/phn-tools/src/env/vector_borne.rs` |
| `housing_conditions_health_guide` | ENV-07 | env | 14 | `crates/phn-tools/src/env/housing.rs` |
| `chemical_exposure_reference` | ENV-08 | env | 14 | `crates/phn-tools/src/env/chemical.rs` |
| `food_access_mapper` | CHR-06 | chronic | 14 | `crates/phn-tools/src/chronic/food_access.rs` |
| `community_bp_screening_toolkit` | CHR-02 | chronic | 15 | `crates/phn-tools/src/chronic/bp_screening.rs` |
| `diabetes_prevention_finder` | CHR-03 | chronic | 15 | `crates/phn-tools/src/chronic/diabetes_prevention.rs` |
| `preventive_services_lookup` | CHR-04 | chronic | 15 | `crates/phn-tools/src/chronic/preventive.rs` |
| `asthma_home_assessment` | CHR-05 | chronic | 15 | `crates/phn-tools/src/chronic/asthma_home.rs` |
| `community_health_assessment_builder` | EQ-08 | equity | 15 | `crates/phn-tools/src/equity/cha.rs` |
| `prioritize_health_issues` | EQ-09 | equity | 15 | `crates/phn-tools/src/equity/prioritize.rs` |
| `analyze_qualitative_data` | EQ-10 | equity | 15 | `crates/phn-tools/src/equity/qualitative.rs` |
| `geocode_and_aggregate_addresses` | EPI-13 | epi | 15 | `crates/phn-tools/src/epi/geocode.rs` |
| `build_logic_model` | PLAN-01 | planning | 16 | `crates/phn-tools/src/planning/logic_model.rs` |
| `write_smartie_objectives` | PLAN-02 | planning | 16 | `crates/phn-tools/src/planning/smartie.rs` |
| `build_evaluation_plan` | PLAN-05 | planning | 16 | `crates/phn-tools/src/planning/evaluation.rs` |
| `find_evidence_based_interventions` | PLAN-03 | planning | 16 | `crates/phn-tools/src/planning/interventions.rs` |
| `apply_behavior_change_theory` | PLAN-04 | planning | 16 | `crates/phn-tools/src/planning/theory.rs` |
| `track_program_performance` | PLAN-11 | planning | 16 | `crates/phn-tools/src/planning/performance.rs` |
| `appraise_evidence` | EBP-03 | ebp | 16 | `crates/phn-tools/src/ebp/appraise.rs` |
| `build_evidence_synthesis_table` | EBP-04 | ebp | 16 | `crates/phn-tools/src/ebp/synthesis.rs` |
| `plan_quality_improvement` | EBP-05 | ebp | 16 | `crates/phn-tools/src/ebp/qi/mod.rs` |
| `search_grant_opportunities` | PLAN-06 | planning | 17 | `crates/phn-tools/src/planning/grant_search.rs` |
| `analyze_funding_announcement` | PLAN-07 | planning | 17 | `crates/phn-tools/src/planning/funding_announcement.rs` |
| `draft_grant_narrative` | PLAN-08 | planning | 17 | `crates/phn-tools/src/planning/grant_narrative.rs` |
| `build_program_budget` | PLAN-09 | planning | 17 | `crates/phn-tools/src/planning/budget/mod.rs` |
| `economic_evaluation_calculator` | PLAN-10 | planning | 17 | `crates/phn-tools/src/planning/economic/mod.rs` |
| `analyze_health_department_budget` | LEAD-01 | leadership | 17 | `crates/phn-tools/src/leadership/budget.rs` |
| `model_staffing_needs` | LEAD-02 | leadership | 17 | `crates/phn-tools/src/leadership/staffing.rs` |
| `assess_foundational_public_health_services` | LEAD-03 | leadership | 18 | `crates/phn-tools/src/leadership/fphs.rs` |
| `phab_accreditation_crosswalk` | LEAD-04 | leadership | 18 | `crates/phn-tools/src/leadership/phab.rs` |
| `build_strategic_plan` | LEAD-05 | leadership | 18 | `crates/phn-tools/src/leadership/strategic_plan.rs` |
| `assess_workforce_competencies` | LEAD-06 | leadership | 18 | `crates/phn-tools/src/leadership/workforce.rs` |
| `board_of_health_meeting_assistant` | LEAD-07 | leadership | 18 | `crates/phn-tools/src/leadership/board_meeting.rs` |
| `draft_standing_orders` | LEAD-08 | leadership | 18 | `crates/phn-tools/src/leadership/standing_orders.rs` |
| `qi_vs_research_determination` | EBP-07 | ebp | 18 | `crates/phn-tools/src/ebp/qi_research.rs` |
| `public_health_ethics_analysis` | POL-05 | policy | 18 | `crates/phn-tools/src/policy/ethics.rs` |
| `nursing_scope_of_practice_guide` | POL-06 | policy | 18 | `crates/phn-tools/src/policy/scope.rs` |
| `track_ma_legislation` | POL-01 | policy | 19 | `crates/phn-tools/src/policy/legislation.rs` |
| `monitor_federal_rules` | POL-03 | policy | 19 | `crates/phn-tools/src/policy/federal.rs` |
| `search_public_health_law` | POL-02 | policy | 19 | `crates/phn-tools/src/policy/law.rs` |
| `draft_policy_brief` | POL-04 | policy | 19 | `crates/phn-tools/src/policy/brief.rs` |
| `health_information_sharing_guide` | POL-07 | policy | 19 | `crates/phn-tools/src/policy/sharing.rs` |

MCP wrappers live in `crates/phn-server/src/server.rs` (Phase 0), `phase1.rs`–`phase12.rs`,
`phase13_ics.rs`, `phase13_coop.rs`, `phase13_hseep.rs`, `phase13_cerc.rs`,
`phase14_water.rs`, `phase14_ejclimate.rs`, `phase14_vectorhousing.rs`, `phase14_chemfood.rs`,
`phase15_screen.rs`, `phase15_prevent.rs`, `phase15_cha.rs`, `phase15_geo.rs`,
`phase16_design.rs`, `phase16_interventions.rs`, `phase16_appraise.rs`, `phase16_qi.rs`,
`phase17_grants.rs`, `phase17_budget.rs`, `phase17_finance.rs`, `phase18_strategy.rs`,
`phase18_workforce.rs`, `phase18_ethics.rs`, `phase19_tracking.rs`, and `phase19_law.rs`. The four
MCP prompts (`investigate_outbreak`, `plan_tabletop_exercise`, `community_health_assessment`, and
`prepare_grant_application`) are in `prompts.rs`; the prompt-list test asserts the exact list. The
registry (tool → plan ID → toolset → phase) is `crates/phn-tools/src/registry/` with one file per
phase from Phase 2 (Phases 0–1 are in `mod.rs`; Phases 13 to 19 have one list per work package;
`PHASES` holds (phase, tools) pairs). Phase 13's shared staff roster code is
`crates/phn-tools/src/epr/roster.rs` (ADR 0009). The offline geocoder's design is ADR 0011.

**Where the Phase 15–19 tools get their data** (`local` means bundled content or your own files and no
network; `snapshot` means a dated bundled copy of an external source; `live` means a public API with
a snapshot or cache fallback unless it says "live only"):

| Tool | Mode |
|---|---|
| `community_bp_screening_toolkit` | local |
| `diabetes_prevention_finder` | snapshot |
| `preventive_services_lookup` | live and snapshot |
| `asthma_home_assessment` | local |
| `community_health_assessment_builder` | live and snapshot |
| `prioritize_health_issues` | local |
| `analyze_qualitative_data` | local |
| `geocode_and_aggregate_addresses` | local |
| `build_logic_model` | local |
| `write_smartie_objectives` | live and snapshot |
| `build_evaluation_plan` | local |
| `find_evidence_based_interventions` | live and snapshot |
| `apply_behavior_change_theory` | local |
| `track_program_performance` | local |
| `appraise_evidence` | local (optional live DOI lookup) |
| `build_evidence_synthesis_table` | local |
| `plan_quality_improvement` | local |
| `search_grant_opportunities` | live and snapshot |
| `analyze_funding_announcement` | local |
| `draft_grant_narrative` | local |
| `build_program_budget` | local |
| `economic_evaluation_calculator` | local |
| `analyze_health_department_budget` | local |
| `model_staffing_needs` | local |
| `assess_foundational_public_health_services` | snapshot |
| `phab_accreditation_crosswalk` | local |
| `build_strategic_plan` | local |
| `assess_workforce_competencies` | local and snapshot |
| `board_of_health_meeting_assistant` | local |
| `draft_standing_orders` | local |
| `qi_vs_research_determination` | local |
| `public_health_ethics_analysis` | local |
| `nursing_scope_of_practice_guide` | local (bundled partial copy) |
| `track_ma_legislation` | live and snapshot |
| `monitor_federal_rules` | live only |
| `search_public_health_law` | snapshot (plus local folder) |
| `draft_policy_brief` | local |
| `health_information_sharing_guide` | local |

### Exit criteria

| Criterion | State |
|---|---|
| Phase 0: the nurse runs the tools from Claude Desktop on Windows and a Mac | **Waiting on the nurse** (`docs/acceptance/phase-0-1.md`) |
| Phase 0: agency IT asked about container runtime approval | **Waiting on the nurse** |
| Phase 1: PHI detector, no row-level data, accessible DOCX | Done (automated tests) |
| Phase 2: all golden tests match published or reference values | Done (R, SciPy, statsmodels, NCHS, Krejcie & Morgan; `crates/phn-epi/tests/golden.rs`) |
| Phase 3: the nurse verifies isolation/exclusion rules for the 30 most investigated diseases | **Waiting on the nurse** (37 diseases, 134 rules; checklist in `docs/acceptance/phase-3.md`) |
| Phase 4: community profile works for all 351 towns live and offline, every figure cited | Offline: done (test loops over all towns). Live: PLACES, SVI, NCBI, CDC feeds verified; **the Census API needs `PHN_CENSUS_API_KEY` from some networks**, so live ACS was not exercised |
| Phase 5: the nurse checks the school requirements against DPH's current document | **Waiting on the nurse** |
| Phase 6: all applicable CDSi test cases pass or exceptions are documented | Done: 1,365 of 1,401 pass (all 1,064 healthy cases); 36 underlying-condition cases documented in `crates/phn-cdsi/tests/fixtures/cdsi_known_exceptions.json`; report in `docs/cdsi-test-results.md` |
| Phase 7: Pilot 2 with school nurses, after the nurse reviews the school content | **Waiting on the nurse** (checklist in `docs/acceptance/phase-7.md`) |
| Phase 8: the nurse reviews the milestone, lead, and mandated reporting content | **Waiting on the nurse** (`docs/acceptance/phase-8.md`) |
| Phase 10: the nurse validates screening cutoffs, safety messages, and instrument license terms, and spot-checks the directories | **Waiting on the nurse** (`docs/acceptance/phase-10.md`) |
| Phase 13: Pilot 3, a tabletop exercise designed with EPR-08, after the nurse reviews the Phase 13 content | **Waiting on the nurse** (`docs/acceptance/phase-13.md`) |
| Phase 14: environmental and climate tools work for any town, live and offline, every figure cited | Offline: done (snapshots for every source; automated tests). Live: SDWIS, the EEA PFAS API, OpenFEMA, and DPH's arbovirus workbook were exercised once by ignored live tests or by hand. Content review: **waiting on the nurse** (`docs/acceptance/phase-14.md`) |
| Phase 15: prevention programs and the full community health assessment workflow (plan goal; the plan also names a MAPP 2.0 prompt, a qualitative pipeline with name scrubbing, and an offline geocoder) | Code and automated tests done, every tool in the leak test. Geocoder: 99.89% of 20,328 real MassGIS addresses fell in the same block group as MassGIS's own layer, and eight public buildings match (golden tests). The plan's USPSTF API adapter and National DPP registry snapshot were **not built** (hosts blocked); the priority extractor for CHNAs was never run on a real CHNA. Content review and acceptance script: **waiting on the nurse** (`docs/acceptance/phase-15.md`) |
| Phase 16: the planning and evaluation workflow and the evidence-appraisal chain (plan goal) | Code and automated tests done. Run chart rules match an independent Python computation, the Anhøj limits match the published table, and the QI and evidence workbooks' formulas were recalculated in LibreOffice (not in Excel). The Crossref adapter was **never reached live**; the Community Guide and *What Works for Health* could not be read. Content review and acceptance: **waiting on the nurse** (`docs/acceptance/phase-16.md`) |
| Phase 17: find funding, respond to it, and manage the money (plan goal) | Code and automated tests done. The budget workbook's formulas matched an independent computation (341 cells, 0 differences, LibreOffice); the economic calculator reproduces CDC's MMWR 2024;73(31) Table 2; the COMMBUYS live parse matched all 147 bids of the real 2026-11-09 PDF. **The federal Simpler.Grants.gov search never ran against the live service** and the announcement reader was not tested on a federal NOFO. Content review and acceptance: **waiting on the nurse** (`docs/acceptance/phase-17.md`) |
| Phase 18: tools for running the department and meeting standards (plan goal) | Code and automated tests done; every quotation in the new policy and scope content is machine-checked against the fetched text. PHAB's own document and the competency statements could not be read. Content review and acceptance: **waiting on the nurse** (`docs/acceptance/phase-18.md`) |
| Phase 19: policy tools and a production-ready v1.0 (plan goal) | Policy tools: code and automated tests done (countdown golden-tested against Python date arithmetic; quotations machine-checked). **The Federal Register and Regulations.gov adapters were never run live** (tested against mock servers). Hardening: the threat model and administrator guide are in `docs/security/threat-model.md` and `docs/admin-guide.md` (written separately; not re-verified here); the content review sweep is `docs/content-review-sweep.md`; the user guide text covers all 148 tools but the Word file must be rebuilt (`node docs/user-guide/build-user-guide.js`). Whether the dependency and container scans, image size, and startup time checks the plan lists were run is not recorded here. Acceptance (including three hardening checks): **waiting on the nurse** (`docs/acceptance/phase-19.md`) |
| Phases 7–12: calculators match reference values | Done: CDC 2000 growth percentiles (relative error < 1e-7), CDC extended BMI, WHO monthly tables, screening scores (50 cases), readability formulas, Erlang C, 40 CFR 58 App. G AQI example, CASPER survey intervals (samplics) |
| Every phase: acceptance script run by the nurse | **Waiting on the nurse** (`docs/acceptance/phase-*.md`, Phases 0–19) |

## 2. Open items

### Needs a person (do not attempt to complete these yourself)

1. **Review all bundled content** (every file is `status: pending`, review due 2027-02-01; **CI
   starts failing after that date** until each `meta.review` is updated). Files, roughly in
   pilot order:
   - `content/cd/reportable_conditions.yaml` (re-drafted 2026-11-07 from the 105 CMR 300.000
     text; 102 high- and 6 medium-confidence entries),
   - `content/cd/disease_rules.yaml` (37 diseases, 134 rules), `measles_exposure.yaml` (based on
     the 2013 ACIP statement), `infection_control.yaml`,
   - `content/vax/*.yaml` (schedules, recommendations by body, school requirements, coverage
     thresholds, products, storage and handling, clinic planning, confidence guide, CDSi
     interpretation choices),
   - `content/equity/language_access.yaml` (the HHS 2003 safe-harbor entry is low confidence:
     hhs.gov and federalregister.gov were unreachable),
   - `content/data_sources.yaml`,
   - Phases 7–12 (each phase's checklist is in `docs/acceptance/phase-N.md`). Items marked low
     confidence or unverified, which need the most attention:
     - `content/school/` (8 files): hearing pass and rescreen criteria come from DPH's 2007 manual
       (the current Pure Tone protocol is on a blocked host, and the manual says both 1 and 2 weeks
       for the rescreen); whether the July 15 Postural Final Report is still collected; the BMI
       reporting route (2019 memo); IHP goals and 504 suggestions are a nursing template; the
       annual report's field names are descriptive (DPH does not publish its Status Report form).
     - `content/mch/`: the corrected-age rule (until 24 months; no CDC source, AAP blocked); jaundice
       items (the AAP 2022 guideline was unreachable); the 51A newborn substance-exposure item
       (§51A(a) and DCF's guide disagree); DPH's pages disagree on capillary confirmation timing
       for 5–9 µg/dL (2 vs 3 months; both shown). `content/equity/benefit_eligibility.yaml`: the
       SNAP effective date is assumed to be 2026-10-01.
     - `content/core/omaha_system.yaml` (signs and symptoms not shown in the website's case studies,
       and rating labels 4–5, are from memory of the 2005 book) and `intervention_wheel.yaml` (MDH's
       site was unreachable; the definitions of four interventions and of the three levels of practice
       are from memory); `content/cd/tuberculosis.yaml`
       (no source gives a 3HR completion window, so it is empty);
       `content/equity/newcomer_health.yaml` (ORI pages disagree on Refugee Medical Assistance: 8 vs
       4 months).
     - `content/bh/mental_health_screening.yaml` (EPDS cut-points and C-SSRS risk levels medium;
       license terms for PHQ medium, EPDS low, C-SSRS unverified); `content/aging/advance_care_planning.yaml`
       (**MOLST is unverified**: the old program domain molst-ma.org no longer belongs to the
       program, and no mass.gov MOLST page was found; the proxy template is drafted from c.201D);
       `content/aging/protective_services.yaml` (651 CMR 5.00 and DPPC's site not read).
     - `content/bh/substance_use_resources.yaml` (fentanyl test strip legality is low confidence);
       `content/comms/campaigns.yaml` (**every platform limit and observance date is low
       confidence**); postvention templates are this project's own wording (the toolkit's license
       does not allow adaptation without EDC's permission).
     - `content/epr/dispensing.yaml` (**all station times and shares are tool defaults**),
       `functional_needs_shelter.yaml` (intake questions are drafts; staffing ratios 50/10/25 are
       tool defaults), `hva.yaml` (the scoring method is this project's own), `content/env/heat.yaml`
       (activation steps and outreach scripts are drafts).
   - Phase 13 (`docs/acceptance/phase-13.md` lists each file's checks):
     - `content/epr/ics_forms.yaml`: form titles, block numbers, and labels were checked against
       FEMA's v3/v3.1 PDFs; the 213RR instructions and every incident suggestion are drafts.
     - `content/epr/continuity.yaml`: **FEMA's Continuity Guidance Circular and plan template could
       not be retrieved** (www.fema.gov returns 403), so the element checklists and phases are
       medium confidence; tiers, recovery times, minimum staff, and TT&E frequencies are tool
       defaults; town counsel must confirm the delegations.
     - `content/epr/capabilities.yaml`: **the PHEP → FEMA core capability mapping is this project's
       own** (no official crosswalk found); three FEMA names follow the National Preparedness Goal
       (not retrievable). `hseep.yaml`: planning-meeting weeks, schedule lengths, the hotwash
       agenda, and the 30/45/60-day after-action dates are tool defaults.
       `exercise_scenarios.yaml`: **the Pilot 3 scenarios are fictional drafts** to vet with the
       planning team and DPH.
     - `content/epr/cerc.yaml`: the mass dispensing set (low confidence; depends on the POD plan);
       the boil water set follows CDC only (MassDEP's boil order pages returned 404); measles FAQs
       on infants and exclusion (medium).
   - Phase 14 (`docs/acceptance/phase-14.md` lists each file's checks):
     - `content/env/drinking_water.yaml`: the federal PFAS status is **time-sensitive** (EPA's
       May 18, 2026 proposals were expected to be final by the end of 2026; re-check);
       `content/chronic/fluoridation.yaml`: CPSTF's position is CDC's description of it (medium),
       the "non-adjusted" status wording is inferred, and the board of health discussion points
       are drafts; ADA, ASTDD, AAPD, WHO, HHS, and CPSTF's own page could not be fetched and are
       listed as not included.
     - `content/env/environmental_justice.yaml`: the MEPA EIR item comes from the MEPA guide page
       (301 CMR 11.06(7)(b) and the two MEPA EJ protocol PDFs were not fetched); the EJScreen
       column list is EPA's file as copied in the EJAM repository (medium).
       `climate_vulnerability.yaml`: **all flagging thresholds are tool defaults** and the
       ZIP-to-town weighting of Heat and Health Index values is this project's method (medium).
     - `content/env/vector_borne.yaml`: the 2026 cancellation times were read from calendar
       images (medium); `housing_conditions.yaml`: the teaching sheets, letter templates, and
       tenant protection summaries are drafts for legal aid or counsel to read.
     - `content/env/chemical_exposure.yaml`: the "occupational limits are not community limits"
       and "an MRL is not a diagnostic threshold" wording is this project's (medium);
       `content/chronic/food_access.yaml`: how to get the SNAP retailer file and the HIP vendor
       list is **low confidence** (the USDA and DTAFinder pages were blocked).
   - Phases 15–19 (all `status: pending`, review due 2027-02-01; `docs/acceptance/phase-N.md` has
     the checks and `docs/content-review-sweep.md` lists every file and snapshot with its
     confidence counts). Items marked **LOW** are low confidence, this project's own drafts, or
     unverified, and need the most attention. Where a source could not be read, the tool says so.
     - Phase 15 (chronic disease prevention and community assessment):
       - `community_bp_screening_toolkit` (`content/chronic/bp_screening.yaml`): **LOW: the five
         referral tiers, the any-reading severe rule, the emergency symptom list, two technique
         steps (cuff size and record keeping, marked "not verified"), the log columns, letters,
         handout, and plausibility limits are this project's drafts**; decide timing and symptoms
         with the medical director, whether severe range triggers on any reading or the average,
         and whether letters name the reading. The 2025 AHA/ACC guideline text itself was not
         read (stage and severe-range wording is medium, from open-access papers; 2026
         corrections not reviewed).
       - `diabetes_prevention_finder` (`content/chronic/diabetes_prevention.yaml`, snapshot
         `ma-dpp-programs`): confirm reuse terms for the paraphrased test; CDC's BMI handling is
         unverified; Medicare DPP eligibility not read; no MassHealth statement (no source);
         DPH's list is undated, 56 of 73 contacts were dropped, one ZIP code (Lanesborough,
         02137) looks wrong; CDC's registry was unreachable (decide whether to unblock it).
       - `preventive_services_lookup` (`content/chronic/preventive_services.yaml`, snapshot
         `uspstf-ab-recommendations`): check each recommendation's criteria against the Task
         Force's pages, especially the ten age ranges marked `assumption` (tobacco prevention in
         youth, BRCA, hepatitis B, PrEP, syphilis, STI counseling, intimate partner violence,
         folic acid, children's BMI interventions); the "confirm" notes, the client summaries
         (medium), and the Massachusetts programs (DPH pages show no date). Unverified: whether any
         statement changed after the PubMed record; the A and B list is incomplete (nothing
         before 2015; no C, D, or I).
       - `asthma_home_assessment` (`content/chronic/asthma_home.yaml`): checklist wording is
         shortened from the CDC/EPA/HUD sheet; the control table is DPH's survey adaptation of
         EPR-3 (screening level, medium) and at its boundaries (0.29 and 1.00 rescue uses a day)
         the tool picks the more severe category, a project decision; a prescriber must complete
         the action plan. Unverified: MassHealth coverage of asthma home visits, local programs,
         program phone numbers, the Clearinghouse forms, the ACT link. The nurse may supply
         NHLBI's EPR-3 table.
       - `community_health_assessment_builder` (`content/equity/cha_framework.yaml`, snapshot
         `cms-ma-hospitals`): **LOW: what MAPP 2.0 Phases 2 and 3 contain beyond one line, where
         prioritization and the CHIP fall, NACCHO's tool names, and that MAPP 2.0 centers equity**
         (NACCHO's pages were unreachable; verified through two 2025 articles only); the 13
         sectors, prompts, and outline are drafts. Check the hospital town mapping (Boston
         neighborhoods and villages), try three or four real CHNAs (the priority extractor has
         only been run on synthetic text and one generated PDF), and decide whether to build a
         verified directory of hospital CHNA links and whether to reproduce or only link NACCHO's
         worksheets (license not read).
       - `prioritize_health_issues` (`content/equity/prioritization.yaml`): NACCHO's Hanlon
         worksheet was unreadable (division by 3, seriousness scoring, and anchors are
         unverified; no anchors bundled); **the strategy grid and the nominal group point rule
         are this project's own (low to medium)**.
       - `analyze_qualitative_data` (`content/equity/qualitative_codebook.yaml`): **LOW: every
         code and keyword (17 entries) is an unsourced draft**; decide whether organization and
         place names are kept, whether three participants is the right floor for the agency, and
         whether de-identified transcripts may ever be written by default (off now). Read the
         de-identified files for remaining identifying detail.
       - `geocode_and_aggregate_addresses` (snapshots `ma-geocoder-sample`,
         `ma-geocoder-crosswalk`, `crates/phn-data/data/geocoder_vocab.json`): **LOW: the 18 Boston
         neighborhood names (from general knowledge) and the extra street-type variants (USPS
         Publication 28 was not fetched)**; check the 90% ZIP rule (`zip_min_share`); MassGIS's data
         disclaimer page could not be retrieved, so check its terms before sharing a built index;
         decide whether to publish an image variant or release asset with the full 35.5 MB index.
     - Phase 16 (program planning, evaluation, and evidence):
       - `build_logic_model` (`content/planning/logic_model.yaml`): the Kellogg guide could not
         be fetched; medium: the label "assumptions" and the Outputs column; **LOW (tool
         defaults): six boxes per column, 25 words per box, and the wording-check word lists**;
         decide whether to keep the heuristics and the default Outputs column.
       - `write_smartie_objectives` (`content/planning/smartie.yaml`, snapshot
         `healthy-people-2030-objectives`): the tool says "achievable" and "time-bound" where CDC
         says "attainable" and "time-based" (decide); the originator of SMART was not established;
         spot-check five objectives (one, PHI-D02, has status unknown); ODPHP's site showed a
         banner that it was restored to its January 29, 2025 version.
       - `build_evaluation_plan` (`content/planning/evaluation_framework.yaml`): **LOW (tool
         defaults, 7 entries): question, indicator, and design templates, data source and method
         suggestions, timing by type, default roles, the stage-to-type mapping, and the six-step
         timeline shares**; decide on a 52-week default timeline and on economic questions.
       - `find_evidence_based_interventions` (`content/planning/evidence_sources.yaml`, snapshot
         `cpstf-publications`): the rating key comes from abstract sentences; the 26 topic groups
         are this project's aid; spot-check five papers and the 51 papers typed "recommendation"
         from their titles; the Community Guide, County Health Rankings, and NLM terms of use were
         not read (confirm the snapshot is acceptable before publishing it in an image); decide
         whether to list only explicitly rated papers by default.
       - `apply_behavior_change_theory` (`content/planning/behavior_theories.yaml`): check each
         definition against the original papers (not reachable); the Socio-Ecological Model
         levels (five vs CDC's four); the Diffusion of Innovations entry; **38 "authored"
         strategies (medium to low), the Transtheoretical stage-matched strategies, and "processes
         of change" (low: no description read)**; measure ideas are not validated instruments.
       - `track_program_performance` (no content file): "on track" at 90% of an even pace is this
         project's convention (`on_track_fraction`); does it match funders' rules? Workbook
         formulas were checked in LibreOffice only.
       - `appraise_evidence` (`content/ebp/appraisal_checklists.yaml`, `grade_certainty.yaml`):
         **LOW: CASP's license and checklist list (site blocked); GRADE rate-up levels for
         dose-response and opposing confounding, the two-level cap on rating down, and the wording
         of the four certainty levels**; the 2011 Cochrane tool is superseded by RoB 2 (not read);
         how AMSTAR 2 counts a "partial yes" is unverified; the qualitative questions follow one
         guide; decide on CASP bundling and a level-of-evidence hierarchy. Crossref was never
         reached live (see the decisions below).
       - `build_evidence_synthesis_table` (`content/ebp/evidence_synthesis.yaml`): the PRISMA
         figure could not be read, so the diagram layout is this project's: compare with the
         official template; decide whether to add the "other methods" column and the
         previous-studies box.
       - `plan_quality_improvement` (`content/ebp/quality_improvement.yaml`): **LOW: the
         astronomical-point rule (the tool's own modified z-score screen), "adopt, adapt, or
         abandon" (general knowledge), and the process-map tips**; medium: shift of 6 and trend of
         5 (Perla's paper was read as an abstract only) and the measure type definitions. Confirm
         the SQUIRE paraphrases and license note before handouts. IHI's pages were not read.
     - Phase 17 (grants and finance):
       - `search_grant_opportunities` (`content/planning/grant_sources.yaml`, snapshot
         `commbuys-weekly-bids`): **LOW: the topic keyword lists, the mapping of a local health
         agency to federal applicant types, and which COMMBUYS titles count as grants**; the funder
         directory is not complete; **the federal search was never run against the real service:
         set `PHN_GRANTS_GOV_API_KEY`, try it, and record the first real response**.
       - `analyze_funding_announcement` (`content/planning/announcement_patterns.yaml`): word
         lists are heuristics (medium for Massachusetts RFRs, **LOW for federal NOFOs**); it
         missed the rating words, the 3-year term, and the eligibility section on the one real RFR
         tried and was never run on a federal NOFO; decide whether to flag criteria it cannot
         place and whether to keep 300-character eligibility excerpts in the local files.
       - `draft_grant_narrative` (`content/planning/narrative_templates.yaml`): the skeletons and
         reviewer prompts are this project's drafting aids; confirm they match what reviewers
         reward.
       - `build_program_budget` (`content/planning/budget_reference.yaml`): check the quoted 2 CFR
         rules (the October 1, 2024 values: 15% de minimis, $50,000 subaward, $10,000 equipment);
         **LOW: the SF-424A object class lines 6a to 6k (grants.gov blocked; from a search summary
         of copies) and the funder-check reminders for salary limits, page limits, and escalation**;
         decide whether escalation should default to a non-zero rate.
       - `economic_evaluation_calculator` (`content/planning/economic_evaluation.yaml`): the 3%
         default discount rate is medium (taken from CDC's MMWR 2024;73(31); the Second Panel was
         read as an abstract, and OMB A-94 was not readable); **LOW: the default sensitivity
         ranges (20%; discount rate 0 to 7%)**.
       - `analyze_health_department_budget` (`content/leadership/department_finance.yaml`): medium:
         the $36.98 per capita estimate; **not read: the PHE request for responses and award
         contract (allowable costs, payment, and reporting terms) and the MMA notice**; DPH's PHE
         page gives 54, 56, and 57 for counts, so none is bundled; check the tool defaults (revenue
         group words, salary words, 10-point pace tolerance, minimum of 3 positions, 3-month run
         rate) and that the personnel rule meets agency and union expectations.
       - `model_staffing_needs` (`content/leadership/staffing_model.yaml`): **LOW: every
         productive-hours default except 2,080 is a placeholder with no source (holiday 88,
         vacation 120, sick and personal 64, training 24, non-service 20%)**: replace with agency
         values; no benchmark is bundled (decide whether to supply one).
     - Phase 18 (leadership, accreditation, and professional practice):
       - `assess_foundational_public_health_services` (`content/leadership/fphs_framework.yaml`,
         snapshots `ma-performance-standards`, `ma-shared-service-arrangements`): the **law naming**
         (decision (a) below); the 56 FPHS Headline Responsibilities were copied from DPH's Word
         file and the PHAB/de Beaumont license was not read; the mapping to statute terms and the
         four own items are medium; **the status scale, priority tiers, and shared-service
         questions are this project's drafts (low to medium)**; standard IDs are positional (they
         can shift when DPH changes the workbook); decide whether workforce standards count as
         mandates, whether to add DPH's discretionary board powers as a fifth set, and a "not
         applicable" default for city-only or town-only mandates.
       - `phab_accreditation_crosswalk` (`content/leadership/phab_standards.yaml`): **PHAB's own
         document could not be fetched; the 10 domains and 20 standards come from CDC's 2023
         toolkit (medium); no measure IDs or titles are bundled; the keyword cues and match rules
         are LOW; the adoption month (February 2022) is from a search summary (LOW)**; PHAB's
         license terms were not read. A person must read PHAB's Version 2022 document.
       - `build_strategic_plan` (`content/leadership/strategic_planning.yaml`): **PHAB's strategic
         plan requirements are unverified; SWOT has no source (LOW)**; SOAR rests on one abstract
         and the stakeholder analysis on one review abstract.
       - `assess_workforce_competencies` (`content/leadership/workforce_competencies.yaml`): the
         numbered competency statements and reuse terms were not read, so none are bundled; domain
         names are medium (articles word one nursing domain differently); **LOW: the pairing of
         Health Equity with Cultural Competency and Community Partnership with Community
         Dimensions of Practice**; the priority cut-offs, default target of 3, small-group size of
         3, and training steps have no source; credential matching is keyword based and waivers
         are not modeled.
       - `board_of_health_meeting_assistant` (`content/leadership/open_meeting_law.yaml`,
         `ma_legal_holidays.yaml`): confirm with the Attorney General's Division of Open
         Government and the clerk that whole Saturdays, Sundays, and holidays are removed from the
         48-hour count (medium; no worked example found) and that cancellations need no 48 hours;
         remote participation provisions expire 2027-06-30; 2028 and the two Suffolk-only holidays
         are computed only (town counsel decides whether a Suffolk board counts them); extend the
         holiday list each year.
       - `draft_standing_orders` (`content/leadership/standing_orders.yaml`): the vaccine order
         outline comes from one Immunize.org and one DPH template (medium); the skeleton and
         placeholders are this project's design; no clinical content is bundled.
       - `qi_vs_research_determination` (`content/ebp/qi_research_determination.yaml`): medium: the
         Hastings Center and JAMA 2000 points (abstracts only); the 14 questions and the rules
         that combine answers are this project's synthesis; decide whether the three required
         questions are the right minimum, whether "undetermined" is acceptable, and whether to add
         OHRP's decision charts and the institution's questions (OHRP's QI FAQ and any
         Massachusetts IRB policy were not read).
       - `public_health_ethics_analysis` (`content/policy/public_health_ethics.yaml`): **LOW: the
         APHA four-step approach and "additional considerations" (one paper) and the decision
         profiles, tensions, and ordered options (no source; 5 low entries)**; medium: Kass's
         questions (the full article and the APHA code were not read); check that nothing
         contradicts 105 CMR 300 or DPH guidance.
       - `nursing_scope_of_practice_guide` (`content/policy/nursing_scope_of_practice.yaml`):
         medium: 244 CMR 3.05(5)(d) (PDF layout), the 244 CMR 4.00 entries, AR 25-01's definition
         of "groups", and the Board's decision question 2; check quotations against the page
         images; not read: 244 CMR 5.00 to 8.00, M.G.L. c. 112 sections 74 to 81C except 80B, and
         23 other rulings (listed by title only); decide whether to extend the snapshot (it has no
         refresh builder).
     - Phase 19 (policy and law):
       - `track_ma_legislation` (`content/policy/ma_legislation_tracking.yaml`, snapshot
         `ma-legislation-194`): **LOW: the 18 topic keyword lists (edit them for the agency; they
         match titles only)**; the stage rules are this project's reading of 850 bills (medium):
         confirm against bills you know; the Legislature's terms of use were not found.
       - `monitor_federal_rules` (`content/policy/federal_rules_tracking.yaml`): **LOW: the CDC,
         CMS, HRSA, FDA, SAMHSA, ACF, and FSIS agency slugs (from memory) and the 11:59 pm Eastern
         deadline note**; FNS is medium; the comment outline arranges EPA's tips only; nothing was
         verified against the live APIs (see decision (d)).
       - `search_public_health_law` (`content/policy/public_law_search.yaml`, snapshots
         `ma-law-mgl-c111`, `ma-law-105-cmr`, `ma-law-244-cmr`): spot-check ten sections of each
         source; confirm the 45 full-text c. 111 sections and 20 full-text 105 CMR chapters are the
         right core set; a lawyer has not checked the 14 topic starting-point lists; decide which
         more chapters to bundle (105 CMR snapshot is 966,000 of 1,000,000 bytes), the local
         regulations folder name, and whether to add 105 CMR 130 to 180 and 603 CMR 23.00.
       - `draft_policy_brief` (`content/policy/policy_brief.yaml`): **LOW: the criteria
         `legal_authority` and `evidence_strength` (no source read)**; Bardach's book was not read;
         check that the default criteria suit board of health audiences.
       - `health_information_sharing_guide` (`content/policy/information_sharing.yaml`): medium:
         several provisions (HIPAA public health definition, judicial, research, personal
         representative; FERPA studies and treatment records; Part 2 consent; c. 111 s. 24M; public
         records definition); the logic that picks which regime to read and the cautions are drafts
         for **counsel to review**; not read: the FERPA statute, HHS and Education guidance, 603
         CMR 23.00, Part 2 subpart E and the 2024 rule's compliance dates (none is stated), 45 CFR
         164.508 and 164.520, and several Massachusetts statutes.
2. **Decisions the nurse asked to make** (collected from the phase work):
   - Reportable conditions: should "immediate" follow DPH's call lists or the regulation's
     "immediately, but in no case more than 24 hours"? DPH's lists disagree on LCM, West Nile,
     and tularemia. Is *Candida auris* reportable by any route? Alpha-gal reporting ends
     2027-03-31 unless extended. Adult blood lead reporting is unverified. Confirm Wilmington's
     HMCC region (DPH's 2019 map shows Region 3).
   - Outbreak analysis: small counts in the exposure table show as `<5`, but risk ratios and
     totals can reveal them; decide whether that meets agency policy. Statuses that are neither
     cases nor clearly non-cases (probable, suspect) are excluded under narrow case definitions.
   - School immunization: DPH's May 2026 flyer and its 2026–2027 requirements disagree on the
     MenACWY minimum age (11th vs 10th birthday; the tool follows the requirements document);
     whether the 4-day grace period applies to school requirements (on by default); set the
     annual refresh date; replace clinic-planning defaults marked "tool default".
   - Surveillance: accept the NNDSS 5-year baseline method (as-published year-to-date and
     4-week windows vs the 5-year range) or switch to CDC's historical limits; the 10%
     week-over-week trend rule; confirm that NWSS sites 742, 743, and 752 are the Deer Island
     systems; choose a DPH wastewater source to replace MWRA's Biobot data, which ended July 2026.
   - Disparities: whether ACS-based disparity measures with survey variance are needed;
     whether DPH has a machine-readable town-level births and deaths source.
   - Review the 36 documented CDSi exceptions.
   - School health (Phase 7): DPH 2018 (rest 2–4 days) or CDC 2025 (most children back in 1–2
     days) for time out of school after a concussion; get DPH's current hearing protocol and its
     Status Report and Monthly Activities forms; is the postural report still collected, and how is
     BMI reported now; review the absence-reason and visit-category keyword defaults and the IHP
     goals and 504 suggestions; CDC's two severe-hypoglycemia thresholds (54 vs 55 mg/dL); is a
     July 1 school-year boundary right; the annual report returns year totals and percentages
     without suppression (cells are suppressed): decide whether that meets agency policy.
   - Maternal and child health (Phase 8): corrected age until 24 or 36 months; how to present the
     51A prenatal-substance-exposure conflict; replace the jaundice content with the guideline or
     the agency protocol; keep DPH's combined lead table (Massachusetts rules plus CDC's 3.5 µg/dL
     reference value); add a newborn weight-loss threshold or not; who owns the annual refreshes
     (lead high-risk list, poverty guidelines, WIC, SNAP, HEAP, MassHealth, EI directory, births,
     deaths, PRAMS).
   - PHN practice (Phase 9): should the LTBI calendar offer only DPH's three regimens; a 3HR
     completion window; may the agency use the full AHC HRSN wording (HITS and PRAPARE have their
     own terms); confirm the Refugee Medical Assistance length with ORI; replace the documentation
     structures with agency policy.
   - Aging and mental health screening (Phase 10): EPDS cut-point (9/10 or 12/13); confirm the
     "any non-zero" alert rule for PHQ-9 item 9 and EPDS item 10; license terms for PHQ, EPDS, and
     C-SSRS, and whether to complete Columbia's training; a MOLST source; an official health care
     proxy form or the statute-based template; a verified local warmline.
   - Substance use and communication (Phase 11): the default target reading grade (8); DPH's
     small-number rules for overdose data (no rate below 5 deaths, unstable below 20); the youth
     survey reference groups; how to describe fentanyl test strips; whether to ask EDC for
     permission to use the toolkit's sample statements; the tone of every template.
   - Preparedness (Phase 12): replace dispensing station times with POD drill data, and whether to
     add a dispensing time target; keep the HVA method or align with the HMCC's tool; shelter
     staffing ratios and intake questions against MEMA and Red Cross forms; heat activation steps
     and outreach scripts; the default CASPER household measure.
   - Preparedness II (Phase 13): FEMA-layout ICS forms or FEMA's fillable PDFs; keep the ICS 207 in
     the default IAP; more incident types (water contamination, cyberattack); COOP tiers, recovery
     times, the 40% absenteeism assumption, a devolution partner, and who owns the yearly review;
     keep the PHEP → FEMA mapping or use a DPH crosswalk; planning and after-action timelines
     versus grant deadlines; EEGs per PHEP capability or per FEMA core capability; which scenario
     Pilot 3 uses and who vets it; more CERC hazards, a lower reading grade for crisis messages,
     and who approves translated pre-scripted messages.
   - Environmental health and climate (Phase 14): the 90% share of residents on community water
     below which private well guidance is shown; whether to add the fluoridation positions that
     could not be fetched, and the order of bodies (DPH first); spot-check the CDC-to-town
     matching in `snapshots/ma_fluoridation_systems.csv` (CDC lists Hingham's Weir River system
     twice with different answers); the climate profile's flagging thresholds; confirm MVP
     designation with the regional coordinator (Chester and Warwick are missing from mass.gov's
     regions page); whether a FEMA flood-zone area by town is needed (NFIP claims are used
     instead; MassGIS's 202 MB `nfhl.zip` is reachable); children's DEET (DPH's tick page says
     10–15%, its mosquito fact sheet 30% or less); whether DPH's 2021 school repellent guidance is
     still current; whether to name a regional poison center and which contacts each chemical
     scenario lists; the 50% land-area rule for carrying the 2019 food access map to 2020 tracts.
   - **Found by the coordinator while folding in Phases 15–19 (2026-11-13), need the nurse:**
     - (a) **Name and year of the state public health law.** The plan calls it the "2024 SAPHE
       2.0" law. `content/leadership/fphs_framework.yaml` (strategy package) says mass.gov and
       G.L. c. 111, s. 27D show the SAPHE Act was enacted in 2020, the grants are now the Public
       Health Excellence (PHE) program, and no page used "SAPHE 2.0". But
       `content/leadership/department_finance.yaml` (finance package) cites the Massachusetts
       Association of Health Boards' "SAPHE 2.0 Guidance Document" on mass.gov and
       states "The SAPHE 2.0 law (Chapter 238 of the Acts of 2024) requires DPH and DEP to fund
       boards of health..." (high confidence). The two content files disagree;
       confirm with DPH's Office of Local and Regional Health and decide what the tools call it.
       The PHE counts also differ: the strategy content reads 54 arrangements and 322
       municipalities, while the finance content says DPH's page gives 54, 56, and 57 and bundles none.
     - (b) **`ledger_column` exception** in `crates/phn-tools/src/leadership/budget.rs`: the PHI
       detector flags any header containing "account" or "acct", which refused every general
       ledger's Account columns, so the budget tool accepts a column whose only finding is that
       header word (values are still scanned; account codes are never returned). Confirm the
       exception or change the detector's header rule.
     - (c) **Personnel totals.** `build_program_budget` (PLAN-09) returns category totals, so with
       one position the personnel total is that person's salary (a caveat says so), while
       `analyze_health_department_budget` (LEAD-01) hides a personnel row backed by fewer than
       three positions. Decide whether PLAN-09 should withhold or round a one-position total.
     - (d) **Live adapters never run.** Grants.gov (Simpler.Grants.gov), the Federal Register, and
       Regulations.gov were blocked from the build environment, so they were tested only against
       local stand-ins (Crossref also; see below). From a network that reaches them, run
       `cargo test -p phn-data -- --ignored` and
       `cargo test -p phn-tools --test phase19_tracking_mock -- --ignored`, test the grant search
       with a real `PHN_GRANTS_GOV_API_KEY`, call `appraise_evidence` with a DOI, and fix any field
       or filter that differs.
     - (e) **Sources read only as PubMed or PubMed Central abstracts or secondary articles**
       because the publisher or organization sites were blocked, so check wording against the
       originals: USPSTF statements (`preventive_services_lookup`); Community Guide papers
       (`find_evidence_based_interventions`); the 2025 AHA/ACC guideline through open-access
       papers (`community_bp_screening_toolkit`); MAPP 2.0 and the Hanlon method
       (`community_health_assessment_builder`, `prioritize_health_issues`); behavior change
       theories (`apply_behavior_change_theory`); GRADE series, Core GRADE, Cochrane 2011,
       CONSORT, ROBINS-I, STROBE, AMSTAR 2, and Perla's run chart paper (`appraise_evidence`,
       `plan_quality_improvement`); the Second Panel on Cost-Effectiveness
       (`economic_evaluation_calculator`); the competency frameworks
       (`assess_workforce_competencies`); Kass, the APHA code, and the QI ethics papers
       (`public_health_ethics_analysis`, `qi_vs_research_determination`); and the Eightfold Path
       (`draft_policy_brief`).
   - **Phase 15–19 decisions listed in the per-tool bullets above**, plus: whether to add a role
     profile for home visiting and community nursing (the `mch` and `aging` toolsets are still in
     no profile), and whether the new `planning`, `leadership`, and `policy` toolsets belong in
     more profiles than `phn_leader` (the guide's Table 3 says only `phn_leader` has them).
   - Profiles: the `mch` and `aging` toolsets are in no role profile (they are on when no profile
     is set). Decide whether to add a profile for home visiting and community nursing.
   - **Peer review (2026-11-11).** Decided by the nurse: keep building toward Phase 14 rather than
     pause for the review backlog; roster contacts are filled only on an exact full-name match.
     Implemented as a conservative default, to confirm or change: the C-SSRS screener reports
     "risk level not yet determined" (with same-day steps) when deciding items are unanswered;
     SPC charts from local files withhold the center line and limits when a period is
     suppressed; outbreak median onset and exposure window are withheld below the suppression
     threshold; a Markdown `---` no longer starts a new page; CSV cells that look like formulas
     get a leading apostrophe; refrigerator readings in °F are judged against 36–46 °F; NWS alerts
     are never shown from a cached copy more than 60 minutes old; the container runs on Eastern
     time (`TZ=America/New_York`). Still to decide:
     - Privacy policy: enforce an outbound host allowlist in code, or rely on the agency
       firewall (`project_plan.md` §5.5 now says it is not enforced); `read_local_file`'s default
       redaction level for documents outside the policies folder (standard or strict); whether
       labels of suppressed categories may be shown; client names in file names returned by
       listings; whether counts of letters and plans by family language need suppression;
       whether suppression must hold across groups and totals in one response (fixed for HRSN,
       screening batches, and school reports); outbreak well-person cells and "percent of cases
       exposed"; TB expansion statements about single contacts; after-action notes that contain
       identifiers (warn, mask, or refuse); feedback-form means beside suppressed counts;
       exact small counts in epidemic-curve images; very broad identifier headers ("Insurance
       type", "Birth weight"); shuffling rows before de-identification; limited data set
       geography.
     - Statistics: dropping zero-population age groups; automatic test choice (n ≥ 30 rule,
       Shapiro–Wilk); Yates correction as the primary 2×2 chi-square; an RSE > 30% unstable-rate
       rule; a minimum sigma for EARS C3; Levin's attributable fraction for outbreak cohorts.
     - Clinical and program: MassHealth's 5-point income disregard and counting an unborn child;
       CDC minimum ages beyond DPH's wording in the school checker; the hepatitis B 2-dose
       adolescent schedule; an "in process" school status; whether an exemption overrides
       "needs review"; ungraded classrooms; measles exclusion for immunocompromised contacts with
       documented doses; LTBI directly observed therapy on weekdays only; EPDS answer-entry mode;
       newcomer screening age rules (in code, not content); the 48-hour dispensing window as clock
       time; dispensing utilization defaults; readability grades for non-English text; town-name
       aliases ("Foxboro", "Manchester"); growth extreme-value cut points; undated screening
       results; a school calendar for absenteeism; SMART-objective heuristics; ICS 214 sorting;
       holding statements by CERC phase; a Controller/Evaluator Handbook and drill MSEL; role
       profiles (`preparedness` has no `vax`, `school_nurse` has no `mch`); ACS vintage switching
       automatically; a runtime warning for overdue content reviews; how maps distinguish
       suppressed from no data; PDF heading levels under the title.
3. **Run the acceptance scripts** for Phases 0–19 on Windows and macOS and send the IT request.
   **Read the user guide** (`docs/user-guide/PHN-MCP-Server-User-Guide.docx`) for accuracy and
   tone before sharing it with other nurses; send corrections as edits to `guide-content.js`.
4. **Publish a release:** bump the workspace version in `Cargo.toml` (it still reads 0.15.0) to 1.0.0, regenerate
   `Cargo.lock`, and tag `v1.0.0` after the Phase 15–19 branch merges, to run `.github/workflows/release.yml`
   (it now scans a local image with Trivy before pushing, builds the SBOM from the source tree,
   then pushes the multi-architecture image, signs it with cosign, and attaches offline
   tarballs). The first two `v0.15.0` runs failed while resolving actions: `trivy-action@0.28.0` has no
   such tag, and `@v0.28.0` needs `setup-trivy@v0.2.1`, which no longer exists. The workflow now
   uses `trivy-action@master` (always current; pin a tag or SHA if upstream breaks a release).
   Delete and re-push the tag after the fix merges (the tag points at the old commit), set the
   GHCR package to public, and watch the later steps (SBOM, signing, tarballs), which have
   still never run. The scan then blocked on CVE-2026-84782 (libssl3 in the distroless base,
   unpatched upstream on 2026-11-11); `.trivyignore` excuses it until 2026-12-11. Remove the
   entry once distroless ships libssl3 3.0.22-1~deb12u2 or later. The CI smoke test now checks that the image reports Eastern time; if it fails,
   copy `/usr/share/zoneinfo` from the builder stage into the runtime image.
5. **Optional API keys** for nurses' installations: `PHN_CENSUS_API_KEY` (needed for live ACS
   from some networks), `PHN_NCBI_API_KEY`, `PHN_OPENFDA_API_KEY`, `PHN_AIRNOW_API_KEY` (the
   AirNow API path is untested: its host was blocked here), `PHN_GRANTS_GOV_API_KEY` (federal grant
   listings; the live path is untested), `PHN_REGULATIONS_GOV_API_KEY` (the Regulations.gov
   cross-check; untested), and `PHN_CROSSREF_MAILTO` (the nurse's own address for Crossref's polite
   pool; Crossref was never reached). Everything works without them (snapshots, public files, or
   lower rate limits).
6. **Annual and periodic refreshes** added by Phases 7–12: Medicare amounts (each November; 2027
   amounts were not out), poverty guidelines (each March), WIC, SNAP, HEAP, and MassHealth limits,
   DPH's lead high-risk list and report, the EI and aging-service directories, births, deaths, and
   PRAMS, DPH overdose data (town data stop at 2022 until the BSAS dashboard can be read), CDC
   provisional overdose counts (monthly), the youth survey (every two years), CMS nursing home
   data, and fast-changing immigrant coverage rules (MassHealth memo EOM 26-10). Spot-check the 63
   towns that no CBHC lists, SHINE's missing Groton row, and Shelburne and Tolland (no meals
   project). Phase 14 adds: DPH's arbovirus risk levels (live in season; rebuild the snapshot
   after each season), DPH's annual tick-borne disease summary, SDWIS and MassDEP PFAS6
   (quarterly), CDC My Water's Fluoride (yearly), the ATSDR MRL table and NIOSH IDLH table,
   FEMA hazard mitigation plans and NFIP data, and the USDA Food Access Research Atlas. Each has a
   builder in `xtask/scripts/build_{water,ejclimate,vectorhousing,chemfood}_snapshots.py` with a
   `--check` mode. Phases 15–19 add: the DPH diabetes prevention program list (undated; rebuild
   with `build_screen_snapshots.py`), the USPSTF snapshot (`build_prevent_snapshots.py`, when the
   Task Force publishes), CMS hospitals (`build_cha_snapshots.py`), Healthy People 2030 objectives
   (`build_design_snapshots.py`), Community Guide papers (`build_interventions_snapshots.py`; the
   weekly CI check does not cover it), the COMMBUYS weekly bid list (replaced weekly; the snapshot
   goes stale quickly; `build_grants_snapshots.py`), DPH's Performance Standards (standard IDs are
   positional; `build_strategy_snapshots.py`) and shared service arrangements (each grant year), the
   Massachusetts legal holiday list (extend each year; `build_workforce_holidays.py` and `FIRST_DAY`
   and `LAST_DAY` in `crates/phn-tools/src/leadership/oml.rs`), the bill snapshot
   (`build_tracking_snapshots.py`; status is as of 2026-11-13), the three law snapshots
   (`build_law_snapshots.py`), and the geocoder sample, crosswalk, and any installed full index
   (`build_geocoder_index.py`; changing its vocabulary means rebuilding every installed index).

### Known limitations and technical debt

- **HMCC regions** come from the 2005 MassGIS layer with one override (Wilmington → Region 3, from
  DPH's 2019 map, in `xtask/src/snapshots.rs`); the current HMCC locator (arcgis.com) was blocked.
- **School districts** come from a 2019 MassGIS layer; newer regionalizations are missing.
- **PDF output** is single-column US Letter without kerning or italics; scripts outside DejaVu Sans
  show as `?` in PDF/SVG/PNG (the tool recommends DOCX for them).
- **No OCR**; **PHI detection is heuristic** (tested against the synthetic corpus).
- **The policy index** and the DPH Guide index rebuild completely when files change.
- **Joinpoint:** locations are chosen by BIC on a grid (NCI's program uses permutation tests),
  and intervals are conditional on the locations.
- **The DPH Guide corpus** omits the *S. pneumoniae* chapter (a legacy .doc file); tables are
  flattened and flowcharts omitted; many chapters date from 2016–2018.
- **Medication data has no offline copy:** `lookup_medication_information` lists what it could not
  check when offline.
- **DPH school survey data** has rates without enrollment, so town figures are unweighted; DPH's
  "N/A" means a survey returned with zero students.
- **CDC Stacks** is offered as a search link only (its search page returns 403 to automated
  requests; OAI-PMH has no topic search). NNDSS annual tables now exist only as PDFs on Stacks, so
  year-end weekly totals are used instead.
- **CDC respiratory dataset IDs change between seasons**; check them each season
  (`crates/phn-data` surveillance adapters and `xtask/src/surveillance.rs`).
  `build-surveillance-snapshots` has no `--check` mode yet.
- **The CDSi engine** fails 36 underlying-condition test cases (documented); the healthy suite passes.
- The variance test is labeled "Levene (median-centered)" in tool output: the eponym "Brown"
  (Brown–Forsythe) matches a seeded surname in the leak tests. Avoid common surnames in fixed
  output text.
- **Phases 7–12 limits:** business-day deadlines (concussion, TB contacts) skip weekends but not
  holidays; absenteeism detection needs at least 8 school days; Omaha, Intervention Wheel,
  documentation, passive-voice, jargon, and safe-messaging checks are keyword or pattern based;
  readability grades can differ from Word's by about one grade; HRSA health center data has no
  services, languages, or hours; Council on Aging phone numbers are not bundled (ArcGIS and
  mcoaonline.org blocked); MCH-05, MCH-06, and the older-adult detail tables use snapshots only;
  24 towns (Monson–Northampton, including New Bedford) show 2023 lead values (a page is missing
  from DPH's 2024 report); ACS year-built bins do not split at 1978; FEMA's National Risk Index
  could not be reached (the HVA tool reads a table the nurse downloads, with unverified column
  names); HeatRisk depends on WPC's experimental image format; emPOWER does not expose its data
  month, and town totals are allocated from ZIP codes; NWS alerts are never served from a
  snapshot; the FindTreatment.gov API takes `sAddr` as "lat,lng" although its documentation says
  "lng,lat"; program and syringe-service maps are links only (Tableau and Google My Maps blocked).
- **Phase 13 limits:** ICS forms are FEMA-layout facsimiles, not FEMA's fillable PDFs; the ICS
  207 is an indented table, not a drawn chart; several people with the same position and no name
  resolve to the first roster holder; facility phone numbers typed in a request are not flagged.
  COOP minimum-staff sums overcount (one person can cover several functions) and the plan has no
  PDF output. SMART-objective and finding-wording checks are word-pattern heuristics; seed and
  custom scenarios have placeholder injects; `design_hseep_exercise` is one very long function
  (refactor candidate). The CERC language plan matches ACS language groups ("Thai, Lao, or
  other…") to a translations folder by the first language named; agency translations exist only
  for the pre-scripted sets.
- **Open after the peer review (2026-11-11).** Lower-severity findings not fixed in PR #7:
  - Privacy and robustness: no size limits on input files, DOCX zip members, or sparse XLSX
    ranges (memory); `Sandbox::new_output_file` checks then writes (not create-new); the
    after-action feedback summary returns header text (a headerless file could echo a comment);
    aging local-fill and the disparities error message can return a cell's text; the leak test
    does not assert each row's success; the concussion log's `injuries` total is not suppressed.
  - Data: the content validator does not catch an unregistered YAML file, a `meta.id` mismatch,
    or `reviewed` without a reviewer; empty live results are cached (PLACES, OpenFEMA); the HRSA
    download can exceed the 20-second timeout; the CVX fetch can cache a block page; several
    tools still date citations from the UTC timestamp; PLACES live rows are not checked against
    the bundled release year.
  - Statistics: constant groups and equal paired differences are not refused up front in
    `run_statistical_test`; infinite statistics serialize as null; "Cramér's V = NaN" for a 2×2
    with an empty row; survey-mean `population_size` and `create_spc_chart` denominator lengths
    are not validated.
  - Documents: PDF table rows taller than a page are cut off; epidemic-curve markers can fall
    past the right edge; PDF headings and table headers are listed as manual checks.
  - Tools: college immunization rules ignore full-time status and age; MenACWY grades 7–10 reject
    a dose given after the 10th birthday as "dose 2"; epinephrine band boundaries pick the lower
    band; the CDSi harness matches exceptions by case ID only; the LTBI twice-weekly calendar can
    omit the start date; outbreak zero cells show "—" for the risk ratio; absence reasons ignore
    negation; absenteeism enrollment is overwritten per row and weekend rows count as school
    days; growth batch files assume metric units; `unborn_children` is unbounded;
    language-access groups under 50; county overdose rates under 20 are not flagged; MassHealth
    65+ for households of 3 or more and PACE for couples; PACE and asset constants are in code
    (`aging/coverage.rs`); chair stands with arms or zero stands without age; resource hours like
    "9-8"; emPOWER totals with masked ZIP codes are point estimates; Omaha "potential" problems;
    HVA probability wording and tied ranks; CASPER clusters selected twice count once in the
    variance; unknown placeholders in agency CERC translations; CERC messages with more than three
    supporting facts and duplicate languages; after-action date arithmetic; async tools that
    parse files on the async runtime (Phases 9, 11, 12, and CERC); the image ships an empty
    `/data`; release actions are pinned to tags, not commit SHAs.
  - Dates: the 1900–2100 check covers files and the communicable disease, school,
    immunization, and growth tools, but not `core/practice.rs` `date_arg` (LTBI, newcomer, TB
    arguments) or the COOP, after-action, ICS, press, and social media date arguments
    (`design_hseep_exercise` allows 1900–2200); an outbreak cell with a date and a time reads
    two-digit years with chrono's 69/68 pivot. The nursing home tool does not show the CMS
    adapter's fallback reason.
  - Template folders: tools resolve `/data/templates/<language>/` with three different rules
    (school/CERC, maternal-child health, and `core/practice.rs` used by shelter intake, LTBI, and
    the newcomer checklist); `haitian_creole` works everywhere, codes such as `ht` do not. Unify
    them.
- **Phase 14 limits:** water systems are listed under the town in their PWSID, so a neighbor's
  system that serves part of a town is not shown; SDWIS has no descriptions for Revised Total
  Coliform Rule codes; school and child care lead testing (on the EEA portal) is not included;
  the EEA PFAS endpoint is the portal's own undocumented API. EJScreen and CDC's EJI are not
  bundled (their hosts and every archive mirror were blocked); the EJ tool reads an archived
  EJScreen file the nurse saves. Flood risk uses NFIP claims (insured losses, not risk) instead of
  flood-zone areas. Tick-borne data are county-level (Dukes and Nantucket combined); NNDSS has
  no 2025–2026 Massachusetts babesiosis or anaplasmosis rows. The 2025 food access map (any SNAP
  store) finds few low-access tracts in cities; the 2019 supermarket map is shown beside it.
  There are no AEGL or soil screening values. SNAP retailers and HIP vendors are files the nurse
  downloads or builds (the USDA and DTAFinder hosts were blocked), and the retailer file's column
  names could not be checked.
- **Phase 15 limits:** the geocoder's full street index (35.5 MB, 351 towns) is optional and not
  shipped; Boston has no MassGIS export and is placed from TIGER ranges only; the index builder
  needs Python 3.10 and numpy; not done: reverse geocoding, intersections, named places, spelling
  correction, PO box ZIP codes, ZIP-level output, a progress report (limit 500,000 rows);
  interpolation assumes an even spread along a straight segment; a MassGIS point is a building or
  parcel center; unit-only addresses match at street level; the Python builder and the Rust tool
  share one vocabulary file and 48 test vectors, and the manifest records no vocabulary version.
  The blood pressure log summary cannot cross two variables (small cells could be recovered between
  tables), letters are English only (`/data/templates` is not wired in), and a log's category
  column is ignored. `preventive_services_lookup` has no USPSTF adapter (the A and B list is a
  snapshot, incomplete before 2015 and for any statement PubMed did not return); it applies only
  age, sex at birth, pregnancy, and 26 risk-factor flags (no hysterectomy or organ inventory; it
  does not know weeks postpartum); MyHealthfinder has no offline copy; the tool is one long
  function and its 53 recommendation ids are duplicated in the snapshot and content file (a test
  keeps them equal). Asthma control is screening level (no EPR-3 grid, no age-specific classifier),
  and the home visiting directory is regional with no town mapping or phone numbers. The CHA
  builder could not reach hospital CHNA links or the Attorney General's reports site, its priority
  extraction is pattern based, it points to EQ-04 (disparities) without running it, and a live
  run takes about a minute. Qualitative scrubbing is conservative and heuristic; a person in two
  transcripts counts twice; a transcript with fewer than three speakers can never report a theme;
  there is no OCR for scanned PDFs.
- **Phase 16 limits:** ODPHP has no public Healthy People JSON API that could be found (the adapter
  and builder read each objective's page; a redesign will break them; the 620 KB snapshot could be
  trimmed); subpopulation data are not included. PLAN-01 and PLAN-05 take the nurse's drafted
  components rather than parsing a free-text description, evaluation roles match a roster only
  when exactly one person holds the position, and the Word logic model is portrait only (the
  diagram is not embedded). The evidence tool shows ratings per paper, not per intervention (49 of
  145 papers have rating words; 41 have no abstract), live results carry no MeSH headings, and the
  snapshot is not covered by the weekly CI check. Theory matching is keyword based. The
  performance tool takes wide and long layouts and row counts only (no formulas or merged cells; the
  workbook formulas cover Data rows 2 to 101) and has no seasonal handling. `appraise_evidence`
  guesses the design from words; other designs (diagnostic accuracy, economic evaluation, case
  series, guidelines, mixed methods) have no questions, there is no QUADAS 2, RoB 2, or AGREE II,
  and no Melnyk or Oxford levels; the synthesis table does not read a CSV or XLSX of studies; no APA
  7 formatting or PMID lookup; the PRISMA diagram omits the "other methods" column and the
  previous-studies box; no test parses the workbook formula text (`phn-server` has no zip reader in
  its dev-dependencies). The QI tracker's chart covers only the rows that existed when it was made,
  its formulas assume no blank rows, shift and trend flags appear from the point a run reaches 6 or
  5, the astronomical-point rule is not in the sheet, and `check_qi_tracker.py` is an opt-in
  LibreOffice script; diagrams are laid out from text width with no sub-causes on the fishbone and
  one "no" branch per decision. All Phase 16 and 17 workbook formulas were checked in LibreOffice
  only, not Excel.
- **Phase 17 limits:** the federal grant search never ran live and the opportunity link pattern is
  unverified; COMMBUYS (commbuys.com) was blocked, so the weekly list has no bid type, amount,
  eligibility, or link. The announcement reader was tuned on one Massachusetts RFR and one state
  NOFO; the server's PDF reader splits words and dates and drops some table cells, scanned PDFs
  have no text, and amendments and linked documents are not read. `draft_grant_narrative` reads no
  live community data (passing a profile result into `data_points` would be a good follow-up).
  The budget tool has no salary cap, mileage or per-diem rates, regional cost rates, construction
  line (SF-424A 6g), or inflation index; at most five years and 150 typed lines; the economic
  calculator has no XLSX output. `analyze_health_department_budget` ignores seasonality (straight
  line) and treats a one-time spike as ongoing (run rate), groups revenue by label words (a chart
  of accounts file would be better), needs a flat table with one header row, and has no chart
  output.
- **Phase 18 limits:** PHAB measure text and the competency statements are not bundled; the PHAB
  crosswalk's evidence is keyword based (it does not read dates, approval, or currency) and its
  privacy rules are heuristics; worksheet IDs (PS-xxx-nnn) are positional in DPH's workbook; the
  XLSX worksheet has no drop-downs; the 56 FPHS responsibilities were copied once with no builder or
  `--check`. Credential matching uses keywords (unmatched entries are counted, not named) and
  waivers are not modeled. The Open Meeting Law calculator covers meetings from 2026-01-06 to
  2028-12-31 and does not model remote participation eligibility, the clerk's filing time, other
  statutes' hearing notices, or complaint processes; minutes parsing is label based. The ethics
  tool encodes its rules by question id (a question change needs a code change); scope-of-practice
  matching is keyword based with a trailing-s stem, and its snapshot has no refresh builder. mass.gov's 244 CMR
  PDFs hold every page twice (use `pdftotext -raw` and check quotations against the page images).
- **Phase 19 limits:** the Federal Register and Regulations.gov adapters never ran live. Bills are
  found by title words only; a bill's hearing dates come from history lines, not the hearings
  calendar; the stage is a heuristic (the Legislature publishes no status field). Sponsor and
  amendment-author names are not returned because they collide with seeded surnames in the leak
  test, and the snapshot's dates (2026-03-05, 2026-05-18, 2026-06-11, 2026-07-06) coincide with
  seeded birth dates of `growth_measurements.csv`, so a later tool that returns Legislature dates
  next to that file would trip the same match. The law search matches words (plural endings
  removed), not meaning, and has no phrase search across chapters beyond a bonus for adjacent
  words; regulation text is PDF extraction (tables, forms, and appendices may be missing);
  HIPAA and Part 2 were amended in 2024 and later and the sharing guide cannot say what a court
  has done with the amendments. The law snapshot builder needs `pdftotext` and, for 244 CMR, `pypdf`.
- **User guide upkeep:** the `.docx` is built by `docs/user-guide/build-user-guide.js` (Node.js
  with the `docx` package, plus LibreOffice and `pdftotext` for contents page numbers); it is not
  part of the Rust build or CI. Every new tool needs an entry in `guide-content.js` (a registry
  test enforces this) and a rebuild. Contents page numbers come from LibreOffice's layout; Word
  may paginate slightly differently (right-click the contents, then Update Field).

## 3. How the code is organized

```
crates/
  phn-core     config (env + /cache/phn-settings.toml), Envelope, toolsets/profiles, errors, fuzzy text
  phn-content  embedded YAML content + JSON Schemas (content/), reportable conditions, source registry
  phn-data     HTTP client, SQLite cache, live→cache→snapshot fetch, snapshot store, MA reference lists,
               adapters: Socrata (data.cdc.gov), Census ACS, SVI, PLACES, NCBI E-utilities, CDC feeds,
               boundaries, openFDA/DailyMed/RxNorm, CVX/MVX, NNDSS, respiratory and wastewater,
               HRSA, CMS Provider Data, older adults, FindTreatment.gov, NWS, OpenFEMA, emPOWER,
               AirNow files, HeatRisk, census blocks (PL 94-171), SDWIS/PFAS6/fluoridation (water),
               EJ layer/Heat and Health Index/NFIP/hazard mitigation plans/MVP (ejclimate), DPH
               arbovirus and tick-borne data (vectorborne), NIOSH/ATSDR (chemicals), USDA food access,
               DPH diabetes prevention list (dpp), USPSTF snapshot and MyHealthfinder (preventive),
               offline geocoder (geocoder, with `data/geocoder_vocab.json`), Healthy People 2030
               (healthy_people), Community Guide papers (cpstf), Crossref, grants (Simpler.Grants.gov
               and the COMMBUYS weekly list), DPH performance standards and shared service
               arrangements (strategy), Massachusetts Legislature (malegislature), Federal Register
               and Regulations.gov (fedreg), law corpus (law)
  phn-epi      rates, suppression, standardization, 2×2/Mantel–Haenszel, GLMs (IRLS), trends/joinpoint,
               hypothesis tests, SPC, sample size, aberration detection (EARS, CUSUM, Farrington),
               M/M/c queueing (Erlang C), survey estimation with cluster (Taylor) intervals
  phn-cdsi     CDC CDSi supporting data (XML) and the immunization evaluation/forecasting engine
  phn-local    /data sandbox, parsers, PHI detector, de-identification, profiling, tantivy policy index
  phn-docs     document model, DOCX/PDF/XLSX/CSV/SVG/PNG writers, mail-merge, a11y checks,
               chart renderer and validated palette (ADR 0006)
  phn-tools    one function per tool, by theme (core, cd, vax, school, mch, aging, bh, epr, env,
               chronic, equity, epi, comms, ebp, planning, leadership, policy); localdata
               (guardrails for analyzing local files); charts. Phases 15–19 added
               `planning/` (logic models, objectives, evaluation, interventions, theory,
               performance, grants, budget, economic evaluation, and the shared diagram module
               `planning/diagram.rs`), `leadership/` (budget and staffing, FPHS, PHAB, strategic
               plan, workforce, Open Meeting Law, standing orders), and `policy/` (legislation,
               federal rules, law search, policy brief, ethics, scope of practice, information
               sharing), plus `ebp/qi/`, `ebp/appraise.rs`, `ebp/synthesis.rs`, and
               `ebp/qi_research.rs`
  phn-server   rmcp server: per-phase #[tool] routers, prompts, toolset filtering, resources, main.rs
content/       reference content (YAML) with review metadata; schemas in content/schemas/
snapshots/     dated data snapshots + manifest.json with SHA-256 checksums (about 16 MB, 64 snapshots)
fixtures/      synthetic test corpus, all fictional; seeded_identifiers.json lists every planted identifier
docs/          tools.md, user-guide/ (Word guide for nurses + its generator), adr/ (0001–0011),
               acceptance/ (per phase, 0–19), content-review-sweep.md, security/threat-model.md and
               admin-guide.md (v1.0 hardening), cdsi-test-results.md
xtask/         build-snapshots, build-public-snapshots, build-surveillance-snapshots,
               build-guide-corpus, build-name-lists, build-service-directories,
               build-nursing-home-snapshot, build-older-adult-snapshots, validate-content;
               scripts/ (Python builders for the other snapshots, including the Phase 15–19 ones:
               build_{screen,prevent,cha,geocoder_index,design,interventions,grants,strategy,
               tracking,law}_snapshots.py and build_workforce_holidays.py; most have `--check`)
```

Read these before changing architecture: `project_plan.md` §5, `docs/adr/0001`–`0011`, and
`CONTRIBUTING.md` (definition of done for a tool). Golden-test fixtures in
`crates/phn-epi/tests/fixtures/` come with the R/Python scripts that generated them
(`generators/`); rerun those rather than editing numbers by hand.

### Design rules that must not be broken

- **Every tool returns `phn_core::Envelope<T>`**, with a markdown summary, typed data, sources
  (with dates and mode), caveats, and a privacy block. Problems the nurse can fix are tool-level
  errors (`isError: true`), not protocol errors.
- **Privacy:** never return row-level local data by default; never offer a way to turn redaction
  off; identifiable output goes to `/data/output` only; frequency tables go through suppression.
  Tools that analyze local files use `phn_tools::localdata` (refuses identifier columns, limits
  categories, no minimum/maximum or rows). Every new data tool goes into the leak test in
  `crates/phn-server/tests/mcp_integration.rs`. Documents about a person (51A, protective
  services, health plans, proxies, shelter intake) use template + local fill: the AI gets the
  template; details come from a local file and go only to `/data/output` (ADR 0008).
- **Translations:** ship English only; agency-approved translations go in
  `/data/templates/<language>/`. No bundled machine translations (ADR 0008).
- **Clinical and legal answers** cite sources, state review status, and call `.decision_support()`.
- **Vaccine recommendations** default to DPH, then AAP/ACOG/AAFP, then CDC/ACIP, and always show
  each body's position (ADR 0007).
- **Content changes** must keep `cargo xtask validate-content` green. Do not mark content
  `reviewed` yourself; only the subject matter expert can.
- **Stdout is the MCP protocol.** Log to stderr only (`tracing`).

## 4. Working on the project

### Commands

```sh
cargo test --workspace                                   # unit, golden, CDSi cases, MCP integration (895 tests at 0.15.0; the current count was not recorded)
cargo test -p phn-data -- --ignored                      # live-network tests (run on an open network)
node docs/user-guide/build-user-guide.js                 # rebuild the Word user guide (see docs/user-guide/README.md)
cargo +stable clippy --workspace --all-targets -- -D warnings   # match CI's toolchain
cargo fmt --all
cargo xtask validate-content                             # schemas, review dates, snapshot checksums
cargo xtask build-snapshots --check                      # MassGIS snapshots (needs network)
cargo xtask build-public-snapshots --check               # ACS, SVI, PLACES, boundaries
cargo xtask build-surveillance-snapshots                 # NNDSS, respiratory, NWSS, MWRA
cargo xtask build-guide-corpus --check                   # DPH Guide and fact sheets from mass.gov
cargo deny check && cargo audit
cargo run -p phn-server --bin phn-mcp-server -- check    # needs PHN_DATA_DIR / PHN_CACHE_DIR set
docker build -t phn-mcp-server:dev .
```

### Toolchain

CI uses the latest stable Rust (1.99.1 as of this writing); run clippy with `cargo +stable`. The
MSRV is 1.88.

### Lessons learned

- **Network:** the cloud environment for this repository allows the CDC, Census, DPH (mass.gov),
  NCBI, FDA, NLM, AAP, AAFP, ACOG, EPA, HHS, eCFR, malegislature.gov, immunize.org, SEER, and MWRA
  hosts. Still blocked or failing: hhs.gov (403 from Akamai), federalregister.gov, govinfo.gov,
  arcgis.com, wonder.cdc.gov, data.wastewaterscan.org, the CDC Stacks search page, and the AAP
  PDF host. mass.gov returns 403 to browser-like user agents; plain curl and the server's own
  agent work. The 105 CMR 300.000 page moved to `/regulations/105-CMR-30000-…`.
  Phases 8–12 added api.weather.gov, www.fema.gov (the OpenFEMA API works; its web pages return
  403), hazards.fema.gov (NRI pages now redirect to a blocked FEMA page), files.airnowtech.org,
  www.wpc.ncep.noaa.gov, empowerprogram.hhs.gov, data.cms.gov, www.cms.gov, data.hrsa.gov,
  findtreatment.gov, aspe.hhs.gov, www.fns.usda.gov (now redirects to a blocked host),
  www.health.state.mn.us (returns 403 to automated requests), www.nimh.nih.gov, sprc.org,
  afsp.org, odphp.health.gov, www.omahasystem.org, and www.mcoaonline.com. molst-ma.org was
  added and then removed: it no longer belongs to the MOLST program. Also blocked: DPH's document
  host d2rw76b9nsxu2w.cloudfront.net, www.who.int (use CDC's copies of the WHO tables),
  ftp.cdc.gov, healthychildren.org, phqscreeners.com, cssrs.columbia.edu, 988lifeline.org,
  masshelpline.com, helplinema.org, dppcmass.gov, mcoaonline.org, airnow.gov, www.airnowapi.org,
  Tableau Public, and Google My Maps.
- **Census API** redirects to `missing_key.html` without a key from some networks;
  www2.census.gov summary files and the variables endpoint work without one.
- **NNDSS quirks:** area names are upper case 2022–2024 and title case from 2025; the U.S. total
  is "U.S. Residents" ("TOTAL" adds territories); `year` is text and `week` a number; condition
  names change between years. **NREVSS** repeats a week on each re-post (keep the latest).
- **Reference values:** R's `uniroot` (fisher.test, epitools) is accurate to about 1e-4, so the
  exact-interval golden tests compare tightly with SciPy and loosely with R. NCHS standard
  weights come from the Master List (Statistical Notes 20), not from summed single years.
- **Parallel phase work:** per-phase files (registry, routers, docs sections, leak-test markers)
  kept merges to union conflicts in shared JSON (manifest, seeded identifiers), `Cargo.lock`, and
  xtask `main.rs`. Merge JSON by key, regenerate `Cargo.lock` with cargo. Agents sharing one
  `CARGO_TARGET_DIR` across worktrees picked up each other's stale workspace artifacts (same
  crate names); use separate target directories or touch sources before building.
- **Worktrees for helper agents:** the Agent tool's `isolation: "worktree"` created worktrees
  from an old commit (the remote's default HEAD), not the current branch, so every helper
  stopped. Create the worktrees yourself (`git worktree add -b phase-N-work <dir> <commit>`) and
  give each helper its absolute path; its shell starts in the main checkout, so it must `cd`
  into its worktree in every command.
- **Marker comments and rustfmt:** a comment right after the last element of a `const` array is
  attached to that element's line (`}, // marker`) and the following comments are re-indented,
  so the per-phase markers in `phn-content`'s `ITEMS` conflicted at every merge. Rebuild that
  section by phase after merging, rather than trusting the union.
- **`.gitignore`** is GitHub's Python template: it ignores folders named `env/`, `lib/`, `build/`,
  `downloads/`, `var/`, and others anywhere. `crates/phn-tools/src/env/` and `content/env/` needed
  exceptions; check `git status` when you add a folder with one of those names.
- **User guide values:** `internet` must be `no`, `optional`, or `yes`; anything else breaks the
  Word build (a registry test now checks this).
- **Disk:** the sandbox has a fixed allowance; `target/debug/incremental` grows to several GB.
  Delete it when space runs low. Six parallel builds (one target directory each, about 3–4 GB)
  fit only with `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, and deleting linked test executables
  (`find $CARGO_TARGET_DIR/debug/deps -maxdepth 1 -type f -perm -u+x ! -name '*.so' -delete`)
  between full runs.
- **Usage limits:** all six helpers stopped at once on a 5-hour usage limit; their work was
  saved, and a message to each after the reset resumed it.
- **Phase 13 parallel work:** per-package router files (`phase13_<key>.rs`), registry lists, and
  marker sections limited merge conflicts to `phn-content` `ITEMS`, the registry test, the
  guide's `TOOL_COUNT`, and the acceptance doc; resolve them by union and set the count by hand.
  The registry test assumed one list per phase; `PHASES` now holds (phase, tools) pairs. Markdown
  tables break on marker comments and blank lines; remove markers from tables after merging.
- **Disk for parallel builds:** a full debug `target/` is about 7.7 GB. A debug-free seed target
  (`CARGO_PROFILE_DEV_DEBUG=0`, workspace crates and test executables deleted) is 1.4 GB; copying
  it into each worktree and building with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
  CARGO_BUILD_JOBS=2` let four builds run in about 25 GB.
- **Toolchain:** static.rust-lang.org is reachable; `rustup update stable` brings the local
  toolchain to CI's (1.99.1).
- **Network (Phase 13):** training.fema.gov (ICS forms) and preptoolkit.fema.gov (HSEEP) were
  added and work; www.fema.gov pages still return 403 (Continuity Guidance Circular, National
  Preparedness Goal); MassDEP's boil order pages returned 404.
- **Peer review (2026-11-11):** twelve read-only reviewers (one per area) found about 210
  issues; the same agents then fixed their own findings with strict file ownership and no
  cargo, and the coordinator built, linted, and tested centrally. Eleven agents editing one tree
  without compiling produced a workspace that built on the first try with one clippy error and
  one failing test. Repeated 5-hour usage limits and two container restarts interrupted the fix
  phase; agents resumed from their saved transcripts, and pushing a WIP commit at each stop kept
  the work safe. Shared root causes found by several reviewers at once: chrono's `%Y` reads
  1–4 digits, so "3/5/21" parsed as year 21 (fixed in `phi::parse_date`, `vax/dates.rs`, and the
  line-list parser by rejecting years outside 1900–2100); byte slicing and `String::truncate` on
  non-ASCII text; `f64::signum(0.0)` is 1.0; substring keyword matching; fuzzy matching that
  accepted ties; and suppression applied per row when totals elsewhere revealed the cell.
- **Network (Phase 14):** data.epa.gov (Envirofacts `dmapservice` tables; the older
  enviro.epa.gov pages are blocked), www.ers.usda.gov, nccd.cdc.gov (My Water's Fluoride, through
  ASP.NET form posts), msc.fema.gov, eeaonline.eea.state.ma.us (the EEA Data Portal's PFAS API),
  and wwwn.cdc.gov (ATSDR MRLs and ToxFAQs) were added and work; www.atsdr.cdc.gov and the MassGIS
  S3 downloads work. Still blocked: eji.cdc.gov, gaftp.epa.gov, ejscreen.epa.gov (and
  www.epa.gov/ejscreen is 404), web.archive.org, source.coop, `*.github.io`, github.com release
  pages, zenodo.org, dataverse.harvard.edu, screening-tools.com, arcgis.com and
  gis.data.mass.gov, resilientma.mass.gov, ephtracking.cdc.gov, echo.epa.gov,
  gisportal.ers.usda.gov, www.fna.usda.gov (where www.fns.usda.gov now redirects),
  catalog.data.gov, DTAFinder, www.osha.gov, the poison center sites, ntp.niehs.nih.gov,
  www.cochranelibrary.com, pubmed.ncbi.nlm.nih.gov (web; E-utilities work), www.ada.org,
  www.astdd.org, www.aapd.org, www.thecommunityguide.org, and publications.aap.org (403). eCFR
  web pages redirect to a blocked host; its versioner API works.
- **SDWIS quirks:** `STATE_CODE` is the mailing address (out-of-state systems appear), so filter
  on primacy agency; a town's systems share the PWSID prefix `MA` + MassDEP region digit + MassGIS
  town number; lead and copper sample ids repeat across systems (join on PWSID and sample id).
  The one tribal system (Aquinnah Wampanoag) has EPA Region 1 primacy.
- **Leak-test surnames again:** citing a journal article's first author ("Taylor") failed the leak
  test; cite evidence by journal and year, not author.
- **Phase 14 parallel work:** with per-package marker sections everywhere, the only conflicts were
  the README rows above a shared marker line, `manifest.json`, and `seeded_identifiers.json`. A
  small script that unions the README hunks and merges the JSON lists by key resolved every
  merge; a trial merge of the unfinished branches in a throwaway worktree found the conflicts
  early. Helpers that hit the usage limit had committed WIP at each step and resumed by message.
- **Network (Phases 15–19).** Reachable (some only with plain `curl` or the server's own agent):
  www.cdc.gov (many pages, MMWR, Stacks PDFs by id), stacks.cdc.gov, www.cms.gov, data.cms.gov
  (Provider Data API serves Hospital General Information; use `curl -g` for bracketed
  conditions), www.epa.gov (not www3.epa.gov; asthma pages and the hosted NHLBI action plan PDF),
  odphp.health.gov (MyHealthfinder API v4 with no key, and the Healthy People pages), census.gov
  and www2.census.gov (TIGER2025 `ADDRFEAT`, `BG`, `COUSUB`), the MassGIS S3 bucket (Master
  Address Data town exports for 350 towns, none for Boston), www.mass.gov (pages and
  `/doc/<slug>/download` files, including Excel and Word files whose last-modified headers give
  their dates; the Board's advisory rulings come back as Word files even with a `.pdf` look: check
  the type before `pdftotext`), malegislature.gov (HTML only for laws; its API has a Swagger
  document at `/api/swagger/v1/swagger.json`, no search endpoint, and a 12 MB bill list),
  immunize.org, raw.githubusercontent.com, api.github.com, github.com clones, and NCBI
  E-utilities. The Census geocoder API (`geocoding.geo.census.gov`) returned nothing. Blocked:
  ahajournals.org, professional.heart.org, acc.org, heart.org, nhlbi.nih.gov and
  catalog.nhlbi.nih.gov, medlineplus.gov, europepmc.org, ncbi.nlm.nih.gov and PMC web pages (E-utilities
  work), doi.org, api.crossref.org, dprp.cdc.gov,
  dprpdataportal.cdc.gov (and the old nccd.cdc.gov `/DDT_DPRP/` application returns 404),
  www.uspreventiveservicestaskforce.org and data.uspreventiveservicestaskforce.org, www.ahrq.gov,
  health.gov, healthfinder.gov, ginasthma.org, massclearinghouse.ehs.state.ma.us, naccho.org,
  irs.gov, law.cornell.edu, healthypeople.gov, countyhealthrankings.org,
  cancercontrol.cancer.gov, phf.org and archive.phf.org, quadcouncilphn.org, nursingworld.org,
  astho.org, apha.org, phls.org, ccnpps-ncchpp.ca, wkkf.org, re-aim.org, oregon.gov,
  casp-uk.net, www.bmj.com, gdt.gradepro.org, prisma-statement.org, ihi.org, squire-statement.org,
  bmjqs.bmj.com, journals.plos.org, jamanetwork.com, archive.org, Wikipedia, search.mass.gov,
  api.simpler.grants.gov, simpler.grants.gov, grants.gov (including apply07 and
  files.simpler.grants.gov), HRSA, SAMHSA, ACF, NIH, ACL, IHS, SAM.gov, USAspending, commbuys.com,
  whitehouse.gov, ed.gov, usda.gov, doi.gov, opm.gov, bls.gov, mma.org, phaboard.org,
  essentialelements.naccho.org, health.mn.gov, slocounty.ca.gov, letsgethealthy.ca.gov,
  clearimpact.com, www.sec.state.ma.us and eli.sec.state.ma.us, doe.mass.edu, umassd.edu,
  openstax.org, regulations.gov and api.regulations.gov, and the
  hospital sites tried for CHNA links (plus `massago.hylandcloud.com`). The Federal Register and
  govinfo.gov, already listed above, stay blocked for the new adapters. eCFR web pages are blocked
  but its versioner API works if the request sends `Accept-Encoding: gzip` (`curl --compressed`;
  without it the API returns 406).
- **Reading blocked sources through PubMed.** `efetch db=pmc` returns the full text of open-access
  articles, `db=pubmed` returns abstracts (including USPSTF statements and Community Guide
  papers), and a `"<organization>"[Corporate Author]` search finds an organization's papers
  (`db=books` returns only ids; some publishers, such as Kass 2001, refuse downloads). `esearch
  db=pmc` returns bare numbers (prefix `PMC`). E-utilities return HTTP 429 after a few calls in a
  row. WebFetch cannot reach hosts that the container proxy blocks.
- **Content and fixture pitfalls (Phases 15–19).** YAML 1.1 reads a bare `yes`, `no`, or `n` as a
  boolean and a plain scalar containing `": "` as a mapping; quote them. PyYAML is more forgiving
  than the Rust loader, so test content through the Rust tests (the schema check catches some of
  this). Citing sources by author fails the leak test (seeded surnames), and so do dates that
  coincide with seeded birth dates. mass.gov's 244 CMR PDFs hold two overlapping text layers.
  `rust_xlsxwriter`'s `Formula::set_result` stores a cached value, so viewers that do not
  recalculate still show numbers; workbook formulas were verified by recalculating in headless
  LibreOffice and comparing with an independent computation.
- **Phase 15–19 parallel work:** fourteen helpers, one per work package, with per-package registry
  lists, router files, and marker sections limited merge conflicts to shared JSON (the manifest and
  seeded identifiers), README rows, and `Cargo.lock`. Marker comments inside the README tables
  split the tables and were removed after merging (remove markers from tables at the end). Eight
  helpers building at once on four cores made a single `cargo build` take many minutes, and the
  shared lock on `target/` stalled a second cargo in the same worktree; some helpers ran with
  `CARGO_BUILD_JOBS=1` and ran the full test suite in pieces.
- **Library APIs** (read the source in `~/.cargo/registry/src/` before guessing):
  - `rmcp` 3.9: `output_schema = schema_for_output::<Envelope<T>>()`; per-phase
    `#[tool_router(router = phaseN_router, vis = "pub(crate)")]` blocks merged in
    `PhnServer::new` (`allow_empty` for a router with no tools yet); prompts via
    `list_prompts`/`get_prompt` with `enable_prompts()`.
  - `quick-xml` 0.42, `tantivy` 0.27 (`en_stem` tokenizer improves recall), `reqwest` 0.13,
    `skrifa` as before. rustfmt removes the blank line before a comment marker in `vec!`/array
    literals.

## 5. Suggested next steps (post-1.0 maintenance)

The plan is complete; nothing is scheduled. In rough order:

- **Get the content reviewed and the acceptance scripts run.** The nurse works through the review
  list in section 2 and `docs/content-review-sweep.md` (low-confidence and blocked-source items
  first), makes the decisions listed there, and runs `docs/acceptance/phase-*.md`. Review dates
  are due 2027-02-01 and CI fails after that.
- **Resolve the coordinator's decisions** (section 2): the name and year of the state public
  health law, the `ledger_column` exception, personnel totals in the budget tool, and the live
  tests that never ran.
- **Release:** bump the workspace version to 1.0.0, rebuild the user guide
  (`node docs/user-guide/build-user-guide.js`), merge the Phase 15–19 branch, tag `v1.0.0`, and
  watch the release workflow (section 2, item 4).
- **Pilots:** Pilots 1, 2, and 3 still wait on content reviews (section 2).
- **Maintenance** (`project_plan.md` section 8): the annual and periodic refreshes in section 2
  (item 6), a few items a month from the technical debt list, and a runtime or CI warning for
  overdue content reviews.
- **Open product decisions:** an outbound host allowlist in code, role profiles for home visiting
  and community nursing and for the new toolsets, a nurse-prompt regression set (planned in the
  plan, not built), and whether to publish an image variant with the full geocoder index.

## 6. History

| Date | Change | PR |
|---|---|---|
| 2026-11-07 | Tool brainstorm (172 tools) and phased project plan | #1 |
| 2026-11-07 | Phases 0 and 1: workspace, 11 tools, privacy guardrails, Docker, CI, docs | #2 |
| 2026-11-07 | Fixed CI clippy failure on Rust 1.99 (`as_chunks`); added STATUS.md and CLAUDE.md | #2 |
| 2026-11-08 | Phases 2–6: 36 tools, `phn-epi` engines, `phn-cdsi`, data adapters and snapshots; Phase 0–1 content re-checked against mass.gov | #3 |
| 2026-11-08 | User guide for public health nurses (Word) and a test that keeps it in step with the tools | #4 |
| 2026-11-09 | Phases 7–12: 50 tools (school health, maternal and child health, PHN practice, healthy aging and mental health screening, substance use and communication, preparedness I); version 0.13.0; user guide rebuilt (116 pages) | #5 |
| 2026-11-09 | Phase 13: 5 tools (ICS forms and IAP, COOP, HSEEP exercise design, after-action report, CERC), `plan_tabletop_exercise` prompt, staff roster module and ADR 0009; version 0.14.0; user guide rebuilt (124 pages) | #6 |
| 2026-11-11 | Peer review of Phases 0–13: about 170 fixes (privacy leaks, wrong clinical and statistical results, panics, stale data shown as current), plan and documentation corrected, ADR 0010, Docker time zone, release workflow reordered; version 0.14.1; 822 tests | #7 |
| 2026-11-11 | Phase 14: 8 tools (drinking water, fluoridation, EJ screen, climate and health vulnerability, vector-borne disease, housing conditions, chemical exposure, food access), 16 snapshots with four builders, population count bases fixed; version 0.15.0; user guide rebuilt (135 pages); 895 tests | #8 |
| 2026-11-11 | Release workflow: Trivy action tag `0.28.0` corrected to `v0.28.0` (first release run failed) | #9 |
| 2026-11-11 | Release workflow: Trivy action now `@master` (pinned `v0.28.0` depended on a deleted `setup-trivy` tag; second release run failed) | #10 |
| 2026-11-11 | Release scan blocked on CVE-2026-84782 (libssl3 in the distroless base, no patched base yet; server uses rustls): added `.trivyignore` entry that expires 2026-12-11 | #11 |
| 2026-11-12 | Phase 15 screen: `community_bp_screening_toolkit`, `diabetes_prevention_finder`; `ma-dpp-programs` snapshot | PR: this branch |
| 2026-11-12 | Phase 15 prevent: `preventive_services_lookup`, `asthma_home_assessment`; `uspstf-ab-recommendations` snapshot | PR: this branch |
| 2026-11-12 | Phase 15 cha: `community_health_assessment_builder`, `prioritize_health_issues`, `analyze_qualitative_data`, `community_health_assessment` prompt; `cms-ma-hospitals` snapshot | PR: this branch |
| 2026-11-12 | Phase 15 geo: `geocode_and_aggregate_addresses`, offline geocoder (ADR 0011), sample index and ZIP crosswalk snapshots | PR: this branch |
| 2026-11-12 | Phase 16 design: `build_logic_model`, `write_smartie_objectives`, `build_evaluation_plan`; Healthy People 2030 adapter and snapshot | PR: this branch |
| 2026-11-12 | Phase 16 interventions: `find_evidence_based_interventions`, `apply_behavior_change_theory`, `track_program_performance`; `cpstf-publications` snapshot | PR: this branch |
| 2026-11-12 | Phase 16 appraise: `appraise_evidence`, `build_evidence_synthesis_table`; Crossref adapter | PR: this branch |
| 2026-11-12 | Phase 16 qi: `plan_quality_improvement` | PR: this branch |
| 2026-11-12 | Phase 17 budget: `build_program_budget`, `economic_evaluation_calculator` | PR: this branch |
| 2026-11-12 | Phase 17 finance: `analyze_health_department_budget`, `model_staffing_needs` | PR: this branch |
| 2026-11-13 | Phase 17 grants: `search_grant_opportunities`, `analyze_funding_announcement`, `draft_grant_narrative`, `prepare_grant_application` prompt; `commbuys-weekly-bids` snapshot | PR: this branch |
| 2026-11-12 | Phase 18 strategy: `assess_foundational_public_health_services`, `phab_accreditation_crosswalk`, `build_strategic_plan`; `ma-performance-standards` and `ma-shared-service-arrangements` snapshots | PR: this branch |
| 2026-11-12 | Phase 18 ethics: `qi_vs_research_determination`, `public_health_ethics_analysis`, `nursing_scope_of_practice_guide` | PR: this branch |
| 2026-11-13 | Phase 18 workforce: `assess_workforce_competencies`, `board_of_health_meeting_assistant`, `draft_standing_orders` | PR: this branch |
| 2026-11-13 | Phase 19 law: `search_public_health_law`, `draft_policy_brief`, `health_information_sharing_guide`; three law snapshots | PR: this branch |
| 2026-11-13 | Phase 19 tracking: `track_ma_legislation`, `monitor_federal_rules`; `ma-legislation-194` snapshot | PR: this branch |
| 2026-11-13 | Coordinator: prompt-list test asserts the exact list; fragments folded into `CHANGELOG.md` (0.16.0 to 1.0.0), `docs/acceptance/phase-15.md` to `phase-19.md`, this file, the data source registry (0.3.0), and `docs/content-review-sweep.md`; README, project plan, and user guide updated for 148 tools; version 1.0.0 (not yet released) | PR: this branch |

## Keeping this file current

At the end of every working session, update:

1. **Last updated**, version, and next phase at the top.
2. **Where things stand** and the tool table.
3. **Open items**: add new debt, and remove items that are resolved.
4. **History**: one line per pull request.

Keep it factual. If you were not able to verify something, say so.
