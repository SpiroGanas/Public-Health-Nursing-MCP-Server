# Project status and handoff

A handoff document for whoever picks up this project next, human or AI agent. Read it before
starting work, and **update it before you finish** (see [Keeping this file current](#keeping-this-file-current)).

- **Last updated:** 2026-11-11
- **Current version:** 0.15.0 (Phases 0–14 of [`project_plan.md`](project_plan.md), 110 tools);
  not yet released. PR #8 (branch `ccr-db56717a-c5p1la`) holds Phase 14.
- **Next:** Phase 15, chronic disease prevention and community assessment. Pilots 1, 2, and 3
  still wait on the nurse's content reviews; no phase has yet met the plan's definition of done
  (content reviewed, signed image published, acceptance script run).

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
| 14 | Environmental health and climate (8 tools) | Open; content review needed | PR #8 |
| 15–19 | See the roadmap in `project_plan.md` §6 | Not started | — |

PRs #5 (Phases 7–12), #6 (Phase 13), and #7 (the peer review: about 170 of roughly 210 findings
fixed; the rest are open items in section 2) are merged. PR #8 (branch `ccr-db56717a-c5p1la`)
holds Phase 14: four helpers built one work package each (water, EJ and climate, vectors and
housing, chemicals and food) in separate worktrees, and the coordinator merged them. When it
merges, start new work on a fresh branch from `main`.

### Tools implemented (110 of 148)

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

MCP wrappers live in `crates/phn-server/src/server.rs` (Phase 0), `phase1.rs`–`phase12.rs`,
`phase13_ics.rs`, `phase13_coop.rs`, `phase13_hseep.rs`, `phase13_cerc.rs`, and
`phase14_water.rs`, `phase14_ejclimate.rs`, `phase14_vectorhousing.rs`, `phase14_chemfood.rs`; the
`investigate_outbreak` and `plan_tabletop_exercise` MCP prompts are in `prompts.rs`. The registry
(tool → plan ID → toolset → phase) is `crates/phn-tools/src/registry/` with one file per phase
from Phase 2 (Phases 0–1 are in `mod.rs`; Phases 13 and 14 have four lists each; `PHASES` holds (phase, tools) pairs). Phase 13's shared staff
roster code is `crates/phn-tools/src/epr/roster.rs` (ADR 0009).

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
| Phases 7–12: calculators match reference values | Done: CDC 2000 growth percentiles (relative error < 1e-7), CDC extended BMI, WHO monthly tables, screening scores (50 cases), readability formulas, Erlang C, 40 CFR 58 App. G AQI example, CASPER survey intervals (samplics) |
| Every phase: acceptance script run by the nurse | **Waiting on the nurse** (`docs/acceptance/phase-*.md`, Phases 0–14) |

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
3. **Run the acceptance scripts** for Phases 0–14 on Windows and macOS and send the IT request.
   **Read the user guide** (`docs/user-guide/PHN-MCP-Server-User-Guide.docx`) for accuracy and
   tone before sharing it with other nurses; send corrections as edits to `guide-content.js`.
4. **Publish a release:** tag `v0.14.1` (PR #7 is merged and CI on `main` passed), or wait and
   tag `v0.15.0` after PR #8 merges, to run `.github/workflows/release.yml`
   (it now scans a local image with Trivy before pushing, builds the SBOM from the source tree,
   then pushes the multi-architecture image, signs it with cosign, and attaches offline
   tarballs). No release has ever been tagged; the workflow has never run, so watch it closely
   the first time. The CI smoke test now checks that the image reports Eastern time; if it fails,
   copy `/usr/share/zoneinfo` from the builder stage into the runtime image.
5. **Optional API keys** for nurses' installations: `PHN_CENSUS_API_KEY` (needed for live ACS
   from some networks), `PHN_NCBI_API_KEY`, `PHN_OPENFDA_API_KEY`, `PHN_AIRNOW_API_KEY` (the
   AirNow API path is untested: its host was blocked here). Everything works without them
   (snapshots, public files, or lower rate limits).
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
   `--check` mode.

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
               arbovirus and tick-borne data (vectorborne), NIOSH/ATSDR (chemicals), USDA food access
  phn-epi      rates, suppression, standardization, 2×2/Mantel–Haenszel, GLMs (IRLS), trends/joinpoint,
               hypothesis tests, SPC, sample size, aberration detection (EARS, CUSUM, Farrington),
               M/M/c queueing (Erlang C), survey estimation with cluster (Taylor) intervals
  phn-cdsi     CDC CDSi supporting data (XML) and the immunization evaluation/forecasting engine
  phn-local    /data sandbox, parsers, PHI detector, de-identification, profiling, tantivy policy index
  phn-docs     document model, DOCX/PDF/XLSX/CSV/SVG/PNG writers, mail-merge, a11y checks,
               chart renderer and validated palette (ADR 0006)
  phn-tools    one function per tool, by theme (core, cd, vax, school, mch, aging, bh, epr, env,
               chronic, equity, epi, comms, ebp); localdata (guardrails for analyzing local files); charts
  phn-server   rmcp server: per-phase #[tool] routers, prompts, toolset filtering, resources, main.rs
content/       reference content (YAML) with review metadata; schemas in content/schemas/
snapshots/     dated data snapshots + manifest.json with SHA-256 checksums (about 12 MB, 50 snapshots)
fixtures/      synthetic test corpus, all fictional; seeded_identifiers.json lists every planted identifier
docs/          tools.md, user-guide/ (Word guide for nurses + its generator), adr/ (0001–0010),
               acceptance/ (per phase), cdsi-test-results.md
xtask/         build-snapshots, build-public-snapshots, build-surveillance-snapshots,
               build-guide-corpus, build-name-lists, build-service-directories,
               build-nursing-home-snapshot, build-older-adult-snapshots, validate-content;
               scripts/ (Python builders for the other snapshots)
```

Read these before changing architecture: `project_plan.md` §5, `docs/adr/0001`–`0010`, and
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
cargo test --workspace                                   # 822 tests: unit, golden, CDSi cases, MCP integration
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
- **Library APIs** (read the source in `~/.cargo/registry/src/` before guessing):
  - `rmcp` 3.9: `output_schema = schema_for_output::<Envelope<T>>()`; per-phase
    `#[tool_router(router = phaseN_router, vis = "pub(crate)")]` blocks merged in
    `PhnServer::new` (`allow_empty` for a router with no tools yet); prompts via
    `list_prompts`/`get_prompt` with `enable_prompts()`.
  - `quick-xml` 0.42, `tantivy` 0.27 (`en_stem` tokenizer improves recall), `reqwest` 0.13,
    `skrifa` as before. rustfmt removes the blank line before a comment marker in `vec!`/array
    literals.

## 5. Suggested next steps (Phase 15: chronic disease prevention and community assessment)

- Pilot 3: the nurse reviews the Phase 13 content (section 2), picks the scenario, and runs the
  tabletop with `plan_tabletop_exercise` / `design_hseep_exercise`; feed what the planning team
  changes back into `content/epr/exercise_scenarios.yaml` and the after-action report into
  `build_after_action_report`'s acceptance checks.
- Tag a release (`v0.14.1` now, or `v0.15.0` after PR #8 merges) and watch the first run of the
  release workflow.
- Work through the review's open technical debt (section 2, "Open after the peer review") a few
  items at a time.
- Phase 15 (`project_plan.md`): CHR-02 `community_bp_screening_toolkit`, CHR-03
  `diabetes_prevention_finder`, CHR-04 `preventive_services_lookup`, CHR-05
  `asthma_home_assessment`, EQ-08 `community_health_assessment_builder`, EQ-09
  `prioritize_health_issues`, EQ-10 `analyze_qualitative_data`, EPI-13
  `geocode_and_aggregate_addresses`. The offline geocoder (MassGIS address points and TIGER) is
  the largest item; check image size early. CHR-05 can reuse the Phase 14 housing content
  (`content/env/housing_conditions.yaml`) and EQ-08 the Phase 14 EJ, climate, and food access
  tools. Check whether the USPSTF API and the National DPP registry are reachable first.
- Before Pilots 1 and 2, get the nurse's content reviews and decisions done (section 2). The
  nurse may also want a role profile for home visiting and community nursing (section 2), and
  the `env` and `chronic` groups are now large enough that an environmental health profile may
  help.

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

## Keeping this file current

At the end of every working session, update:

1. **Last updated**, version, and next phase at the top.
2. **Where things stand** and the tool table.
3. **Open items**: add new debt, and remove items that are resolved.
4. **History**: one line per pull request.

Keep it factual. If you were not able to verify something, say so.
