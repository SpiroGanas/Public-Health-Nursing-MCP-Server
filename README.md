# Public Health Nursing MCP Server

An [MCP](https://modelcontextprotocol.io) server that makes AI assistants such as Claude more
useful to public health nurses working for a Massachusetts city or town, a school district, or
the Massachusetts Department of Public Health (DPH).

It runs in a Docker container on the nurse's own computer. An MCP client such as Claude Desktop
starts it over stdio. Tools work with public reference data, Massachusetts reference lists, and
the nurse's own files in a folder mounted into the container. **Files never leave the
container**, tools return aggregates by default, and anything identifiable is written to an
output folder instead of being sent to the AI model.

> **Status: v0.15.0 — Phases 0 to 14 of the [project plan](project_plan.md) (see
> [CHANGELOG.md](CHANGELOG.md)).** 110 of the 148
> planned tools are available. Bundled reference content (reportable conditions, disease rules,
> immunization schedules and school requirements, school health and maternal-child health rules,
> screening instruments, aging and preparedness guidance, ICS forms, COOP, HSEEP, and CERC
> material, environmental health and housing guidance, and the other items listed by
> `get_data_source_status`) is a **draft pending
> review** by the subject matter expert; every answer from it says so. All clinical and legal output is decision support only.

## Tools

### Phases 0 and 1: foundation, local data, and documents

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `get_data_source_status` | CORE-08 | core | Lists every data source with its data date, review status, live reachability, and caveats. |
| `configure_jurisdiction` | CORE-09 | core | Sets the nurse's town (county and HMCC region are derived), school district, board of health phone, suppression threshold, languages, profile/toolsets, and letterhead. |
| `read_local_file` | CORE-04 | core | Lists files in `/data` and extracts text and tables from PDF, DOCX, XLSX, CSV, HTML, Markdown, and text, with identifier warnings and redaction. |
| `search_agency_policies` | CORE-05 | core | Answers "what does our policy say?" from the agency's own policies, with file and section citations. |
| `export_document` | CORE-06 | core | Turns Markdown into accessible DOCX or tagged PDF with letterhead, XLSX, CSV, SVG, or PNG; includes local mail-merge. |
| `calculate_rates` | EPI-01 | epi | Crude and category-specific rates with exact Poisson confidence intervals and small-number flags. |
| `apply_small_cell_suppression` | EPI-06 | epi | DPH-style small-cell suppression with complementary suppression. |
| `profile_local_dataset` | EPI-07 | epi | Describes a local dataset (types, missingness, possible identifiers) without showing identifier values. |
| `deidentify_dataset` | EPI-08 | epi | Writes a HIPAA Safe Harbor de-identified copy (or a limited data set) and reports what changed. |
| `lookup_reportable_condition` | CD-01 | cd | Whether a disease or lab result is reportable in Massachusetts, by whom, how fast, and to whom. |
| `check_document_accessibility` | COMM-05 | comms | Checks DOCX, HTML, and PDF files against Section 508 / WCAG 2.1 AA. |

### Phase 2: epidemiology and biostatistics engine

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `age_adjust_rates` | EPI-02 | epi | Direct age adjustment to the 2000 U.S. standard population (NCHS weights) with gamma-method intervals and comparisons. |
| `analyze_two_by_two` | EPI-03 | epi | Risk and odds ratios, attributable fractions, chi-square and exact tests, and Mantel–Haenszel stratified analysis. |
| `calculate_sample_size` | EPI-04 | epi | Sample size and power for surveys, two-group comparisons, case-control studies, and cluster samples. |
| `compute_standardized_ratio` | EPI-05 | epi | Standardized incidence or mortality ratios (SIR/SMR) with exact intervals. |
| `analyze_trends` | EPI-09 | epi | Annual percent change, joinpoint segments, interrupted time series, seasonality, and a trend chart. |
| `run_statistical_test` | EPI-10 | epi | Chooses and runs the right test (t, rank, ANOVA, chi-square, correlation, regression), checks assumptions, and explains the result. |
| `create_chart` | EPI-11 | epi | Accessible bar, line, dot, and histogram charts (PNG/SVG) with alt text and a data table. |
| `create_spc_chart` | EBP-06 | ebp | Run charts and XmR, p, u, and c control charts with special-cause rules, baselines, and phases. |

### Phase 3: communicable disease I

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `search_ma_disease_control_guide` | CD-02 | cd | Offline search of DPH's Guide to Surveillance, Reporting and Control and DPH fact sheets, with quoted passages, sections, dates, and URLs. |
| `calculate_isolation_and_exclusion` | CD-03 | cd | Isolation, quarantine, and exclusion dates by setting under 105 CMR 300.200 and DPH guidance, with wording and citations. |
| `analyze_outbreak_line_list` | CD-04 | cd | Person/place/time tables, food-specific attack rates with risk ratios, likely vehicles, and the exposure window from a local line list (aggregates only). |
| `generate_epi_curve` | CD-05 | cd | Epidemic curves sized to the incubation period, with strata, exposure markers, and the likely pattern of spread. |
| `plan_contact_tracing` | CD-06 | cd | Infectious period, elicitation window, per-contact dates, and a blank contact-tracking workbook in the output folder. |
| `assess_measles_exposure` | CD-07 | cd | Evidence of immunity, MMR and immune globulin deadlines and doses, exclusion days 5–21, and priority groups. |
| `detect_aberrations` | CD-10 | cd | EARS C1–C3, CUSUM, and Farrington (Noufaily) signals on a count series, with trade-offs explained. |
| `infection_control_for_congregate_settings` | CD-13 | cd | Precautions, PPE, cohorting, and EPA disinfectant lists by pathogen for shelters, schools, child care, long-term care, and day programs. |

### Phase 4: public data and evidence platform

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `build_community_health_profile` | EQ-01 | equity | One-call profile of a town, county, or tract: ACS demographics, language, income, insurance, disability, housing, and education with margins of error, PLACES, and SVI, compared with MA and the U.S. |
| `get_social_vulnerability_index` | EQ-02 | equity | CDC/ATSDR SVI 2022 tract percentile ranks (overall and four themes) for a town, county, or tract. |
| `language_access_profile` | EQ-03 | equity | Limited English proficiency by language with margins of error, checked against the HHS safe harbor (5% or 1,000 persons). |
| `measure_health_disparities` | EQ-04 | equity | Rate ratios and differences with exact intervals, index of disparity, between-group variance, and attributable fraction. |
| `get_places_estimates` | CHR-01 | chronic | CDC PLACES chronic disease and social-needs estimates with 95% intervals for a county, town, tract, or ZCTA. |
| `create_choropleth_map` | EPI-12 | epi | Accessible SVG/PNG maps of towns, tracts, ZCTAs, or school districts with natural-breaks or quantile classes and hatched suppressed areas. |
| `search_pubmed` | EBP-01 | ebp | PICO-based PubMed search (MeSH plus free text, study-type filters) with abstracts, DOIs, and the exact query. |
| `search_cdc_guidance` | EBP-02 | ebp | Newest MMWR reports and HAN notices on a topic, with a CDC Stacks search link. |

### Phase 5: immunizations I

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `get_immunization_schedule` | VAX-01 | vax | Routine schedules with DPH, AAP/ACOG/AAFP, and CDC/ACIP positions side by side, minimum ages and intervals, and contraindications. |
| `compare_vaccine_recommendations` | VAX-02 | vax | Each body's recommendation with dates and sources, highlighting where federal guidance now differs. |
| `check_ma_school_immunization_requirements` | VAX-04 | vax | Massachusetts school requirements for one student or a roster: met, not met, or not yet due, with the earliest valid date. |
| `analyze_vaccination_coverage` | VAX-05 | vax | Ranks schools and towns from DPH's school survey, flags those below herd-immunity thresholds, and compares MA with the U.S. |
| `vaccine_temperature_excursion_helper` | VAX-06 | vax | Steps, reporting thresholds, and contacts for a cold-chain excursion, with an optional worksheet. |
| `lookup_vaccine_product` | VAX-07 | vax | Vaccine products by name, CVX, MVX, or NDC, with VIS editions and storage notes. |
| `plan_vaccination_clinic` | VAX-08 | vax | Staffing, throughput, supplies, anaphylaxis kit and epinephrine dosing, layout, and a reconciliation sheet. |
| `vaccine_confidence_conversation_guide` | VAX-09 | vax | Motivational-interviewing responses to vaccine concerns with cited facts and a reading-level check. |
| `lookup_medication_information` | CORE-07 | core | RxNorm names, FDA label highlights, recalls, shortages, and DailyMed package inserts (reference only). |

### Phase 6: immunization forecasting and surveillance feeds

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `forecast_immunizations_due` | VAX-03 | vax | Evaluates a dose history and forecasts doses due (CDC CDSi engine, ACIP schedule) for one person or, with aggregates only, a file of many people |
| `get_notifiable_disease_trends` | CD-08 | cd | NNDSS weekly counts for Massachusetts, New England, and the U.S. compared with the five previous years (year to date, last 4 weeks, year-end totals) |
| `get_respiratory_and_wastewater_surveillance` | CD-09 | cd | Weekly COVID-19, influenza, and RSV briefing: hospital admissions, ED visits, test positivity, and wastewater levels by Massachusetts site, with data dates |

### Phase 7: school health

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `get_ma_school_screening_requirements` | SCH-01 | school | Required school screenings by grade (105 CMR 200.000, M.G.L. c. 71) with protocols, referral, notification, and reporting rules. |
| `track_school_screenings` | SCH-02 | school | From a local roster: who is due, completion, referral, and follow-up rates by grade, BMI categories, and referral letters in each family's language (written locally). |
| `school_medication_administration_guide` | SCH-03 | school | Cited answers on medication administration in schools (105 CMR 210.000, amended 12/5/2025). |
| `build_individualized_health_plan` | SCH-04 | school | IHP and emergency action plan for diabetes, asthma, seizures, or food allergy, with suggested 504 accommodations; filled locally from a student file. |
| `concussion_return_to_learn_tracker` | SCH-05 | school | 105 CMR 201.000 deadlines, reentry and return-to-play stages, accommodations, and head injury log checks with reentry plans written locally. |
| `monitor_school_absenteeism` | SCH-06 | school | EARS signals for influenza-like or gastrointestinal absences by school, with when to call the board of health. |
| `build_school_health_annual_report` | SCH-07 | school | Year-end statistics from local logs in DPH's published reporting categories, plus a school committee summary. |
| `child_care_health_consultation_checklist` | SCH-08 | school | Health consultant checklist against EEC licensing rules (606 CMR 7.00). |
<!-- Phase 7 rows go above this line. -->

### Phase 8: maternal, child, and family health

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `calculate_growth_percentiles` | MCH-03 | mch | WHO (0-24 months) and CDC (2-20 years) percentiles and z-scores with extended BMI, corrected age, BMI category, and visit-to-visit change; batch files with suppressed counts |
| `childhood_lead_screening_guide` | MCH-04 | mch | Massachusetts lead screening schedule (105 CMR 460.000), DPH high-risk communities, confirmation timing and follow-up by blood lead level, reporting, and a family teaching sheet |
| `mandated_reporter_51a_guide` | MCH-07 | mch | Massachusetts 51A reporting: who, when (call now, written report in 48 hours), what to include, protections, DCF process; fills a written-report draft from local notes into /data/output |
| `check_developmental_milestones` | MCH-02 | mch | CDC milestone checklist for a child's age (2022 revision) with act-early steps, screening ages, Massachusetts Early Intervention referral, and the EI programs for a town |
| `plan_newborn_postpartum_visit` | MCH-01 | mch | Newborn and postpartum home visit plan: feeding and weight, jaundice, safe sleep, car seats, MA newborn screening, and CDC Hear Her maternal warning signs, with a DOCX plan and teaching sheet |
| `estimate_benefit_eligibility` | EQ-06 | equity | Income screen for WIC, SNAP, MassHealth, the Health Safety Net, and heating assistance from household size and income (2026 poverty guidelines; a screen, not a determination) |
| `map_lead_risk` | MCH-05 | mch | Ranks towns or tracts for lead outreach from pre-1950/1980 housing, low income, young children, DPH screening rates and prevalence, and DPH high-risk status, with a choropleth map |
| `maternal_infant_health_profile` | MCH-06 | mch | Births, low birthweight, preterm, prenatal care, infant mortality, and PRAMS indicators by community and race/ethnicity, compared with MA and the U.S. |
<!-- Phase 8 rows go above this line. -->

### Phase 9: PHN practice toolkit and case management

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `plan_tb_contact_investigation` | CD-11 | cd | TB contact investigation per NTCA/CDC: infectious period, contact priorities, deadlines, the 8-10 week retest, DPH TB contacts, and a tracking workbook. |
| `ltbi_treatment_calendar` | CD-12 | cd | Dose calendar, monitoring visits, completion window, and adherence check for 3HP, 4R, 3HR, 6H, and 9H (no dose calculation). |
| `screen_health_related_social_needs` | EQ-05 | equity | Scores the CMS AHC HRSN screening tool by domain and maps positive domains to Massachusetts referral categories; batch mode returns suppressed counts. |
| `omaha_system_coder` | CORE-01 | core | Codes a narrative to Omaha System problems, interventions, and KBS ratings without returning the text. |
| `classify_intervention_wheel` | CORE-02 | core | Classifies activities by the Public Health Intervention Wheel's 17 interventions and 3 levels for workload reports. |
| `nursing_documentation_template` | CORE-03 | core | SOAP, DAR, and narrative templates with required elements, and a check of a local note that never returns its text. |
| `find_community_resources` | EQ-07 | equity | Searches the nurse's resource directory and HRSA health centers by town, service, language, and hours. |
| `newcomer_health_navigator` | EQ-11 | equity | Refugee and immigrant health: CDC domestic screening, DPH/ORI programs, MassHealth coverage by status (October 2026 changes), and shelter resources. |
<!-- Phase 9 rows go above this line. -->

### Phase 10: healthy aging and mental health screening

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `score_mental_health_screen` | MH-02 | bh | Scores PHQ-2/9, GAD-2/7, EPDS, NIMH ASQ, and the C-SSRS screener with published cutoffs and safety alerts (988, brief suicide safety assessment, agency protocol); never diagnoses. |
| `find_crisis_resources` | MH-01 | bh | 988, the Behavioral Health Help Line, mobile crisis, Crisis Text Line, and the town's Community Behavioral Health Center, with language options. |
| `steadi_fall_risk_assessment` | AGE-01 | aging | CDC STEADI screening, TUG, chair stand norms, 4-stage balance, orthostatic BP, and an intervention and home-safety plan. |
| `elder_protective_services_guide` | AGE-03 | aging | Adult Protective Services (60+) and DPPC (18-59) reporting rules, timelines, and a report narrative filled locally. |
| `find_ma_aging_services` | AGE-02 | aging | ASAP, Council on Aging, SHINE, meals, caregiver and dementia supports, and protective and ombudsman contacts for a town. |
| `explain_older_adult_coverage_options` | AGE-04 | aging | Medicare 2026 costs, Medicare Savings Programs, MassHealth 65+, Frail Elder Waiver, PACE, SCO, and One Care with a screen. |
| `nursing_home_quality_lookup` | AGE-06 | aging | CMS star ratings, staffing, infection control citations, and staff and resident vaccination for Massachusetts nursing homes. |
| `map_older_adult_vulnerability` | AGE-05 | aging | Older adults living alone, with disabilities, without a vehicle, with limited English, or in poverty, by tract or town, with a map. |
| `advance_care_planning_toolkit` | AGE-07 | aging | Health care proxy rules (c.201D) and template filled locally, MOLST status, and a community session script and handout. |
<!-- Phase 10 rows go above this line. -->

### Phase 11: substance use, crisis resources, and health communication

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `check_readability` | COMM-01 | comms | Flesch Reading Ease, Flesch-Kincaid, SMOG, and Fog grades; long sentences, CDC Everyday Words jargon, passive voice, and acronyms flagged; CDC Clear Communication Index scored. |
| `get_overdose_data` | MH-03 | bh | DPH town overdose deaths 2015-2022 with rates vs the state, CDC provisional county, state, and U.S. counts by drug, and DPH's 2024 findings. |
| `get_youth_risk_behavior_data` | MH-07 | bh | Massachusetts Youth Health Survey and YRBS indicators (sadness, suicide, bullying, substance use) by sex, grade, race/ethnicity, and LGBTQ identity with 95% CIs, vs the U.S. |
| `find_naloxone_and_harm_reduction` | MH-04 | bh | Naloxone access under the statewide standing order, syringe service approval by town, fentanyl test strips, help lines, and a cited naloxone teaching script (Word handout). |
| `find_substance_use_treatment` | MH-05 | bh | FindTreatment.gov facilities near a town: methadone, buprenorphine, naltrexone, withdrawal management, recovery supports, filtered by MassHealth and language, plus the Substance Use Helpline. |
| `plan_school_postvention` | MH-06 | bh | After a suicide affecting a school: toolkit steps, talking points, staff/student/parent/media templates filled locally, and a safe-messaging check of drafts. |
| `translate_health_material` | COMM-02 | comms | Translation workflow (no bundled translations): language priorities, agency translations, translator brief, glossary and back-translation checks, review worksheet. |
| `create_social_media_campaign` | COMM-03 | comms | Posting calendar tied to health observances, checks of draft posts (length, alt text, hashtags, readability, safe messaging), XLSX calendar. |
| `draft_press_materials` | COMM-04 | comms | Press release, media advisory, talking points, or spokesperson Q&A with CDC CERC checks and a Word export. |
<!-- Phase 11 rows go above this line. -->

### Phase 12: emergency preparedness I: situational awareness and planning

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `get_active_hazard_alerts` | EPR-05 | epr | Current NWS watches and warnings for a town, county, or the state with likely health effects and public health actions, plus FEMA declarations. |
| `model_dispensing_site_throughput` | EPR-02 | epr | POD and mass vaccination site queues (M/M/c): staff per station, waits, bottleneck, capacity, and hours to reach a population. |
| `air_quality_protective_guidance` | ENV-02 | env | Current and forecast AQI from AirNow with actions for sensitive groups, schools, and outdoor events; concentration-to-AQI conversion. |
| `heat_health_action_planner` | ENV-01 | env | 7-day HeatRisk and forecast, heat-vulnerable population, activation steps, and a heat response checklist with cooling centers and outreach scripts. |
| `hazard_vulnerability_assessment` | EPR-06 | epr | Facilitated HVA (probability × severity) seeded with ResilientMass ratings, FEMA declarations, and optional NRI scores, with a workbook. |
| `functional_needs_shelter_intake` | EPR-03 | epr | CMIST shelter intake form, local fill to the output folder, and estimates of support needs and staff for a shelter population. |
| `get_electricity_dependent_population` | EPR-04 | epr | HHS emPOWER counts of electricity-dependent Medicare beneficiaries by town, county, and ZIP code for outage planning. |
| `plan_casper_assessment` | EPR-11 | epr | CDC CASPER cluster sample of census blocks, team materials, and weighted analysis with confidence intervals. |
<!-- Phase 12 rows go above this line. -->

### Phase 13: emergency preparedness II: ICS, plans, and exercises

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `generate_ics_forms` | EPR-01 | epr | FEMA ICS forms (201–208, 213, 213RR, 214, 215A) and an assembled Incident Action Plan in Word/PDF; staff contacts filled locally from a roster file. |
| `continuity_of_operations_planner` | EPR-07 | epr | Board of health COOP plan by FEMA's continuity elements: essential functions with tiers and authority, succession, delegations, sites, records, absenteeism check, TT&E schedule, and pandemic annex. |
| `design_hseep_exercise` | EPR-08 | epr | HSEEP exercise design: SMART objectives mapped to PHEP and FEMA core capabilities, measles, coastal storm, and heat tabletop scenarios, SitMan, EEGs, feedback form, and MSEL. |
| `build_after_action_report` | EPR-09 | epr | HSEEP after-action report and improvement plan from hotwash findings, with corrective-action checks and suppressed feedback-form totals. |
| `crisis_risk_communication_builder` | EPR-10 | epr | CDC CERC message maps (27/9/3 checks), holding statements, FAQs, spokesperson prep, and phase tasks for seven pre-scripted hazards or your own, with a language plan (agency translations or translator briefs). |
<!-- Phase 13 rows go above this line. -->

### Phase 14: environmental health and climate

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `drinking_water_quality_lookup` | ENV-03 | env | Public water systems for a town (EPA SDWIS): health-based violations, lead and copper 90th percentiles, MassDEP PFAS6 results, what residents can do, and private well testing guidance. |
| `community_water_fluoridation_status` | CHR-07 | chronic | Town fluoridation status (DPH 2023, CDC My Water's Fluoride) with the systems behind it, a statewide summary, positions and evidence side by side, M.G.L. c. 111 § 8C, and an optional board of health brief. |
| `environmental_justice_screen` | ENV-04 | env | Massachusetts 2020 EJ block groups by criterion for a town, county, or the state, people living in them, English isolation and language access, SVI context, what designation means (MEPA, DPH vulnerable health criteria), optional archived EJScreen summary, map, and table. |
| `climate_health_vulnerability_profile` | ENV-05 | env | Town profile for MVP and hazard mitigation planning: heat days, tree canopy, impervious surface, NFIP flood claims, social vulnerability, climate-sensitive health, MVP and FEMA plan status, and the most important vulnerabilities, with a DOCX option. |
| `vector_borne_disease_risk` | ENV-06 | env | DPH EEE and West Nile virus risk levels by town (live in season, otherwise the dated last levels) with DPH's actions for each level, tick-borne disease cases and rates by county, and prevention messages for the public, schools, camps, and outdoor workers; optional DOCX notice. |
| `housing_conditions_health_guide` | ENV-07 | env | State Sanitary Code (105 CMR 410.000) guide to mold, pests, heat, hot water, lead, alarms, and trash: code sections, owner and occupant duties, health effects, how to get a board of health inspection, tenant protections, readable tenant teaching sheets, and locally filled request letters. |
| `chemical_exposure_reference` | ENV-08 | env | NIOSH Pocket Guide entry, current IDLH, ATSDR minimal risk levels, and ToxFAQs links for a chemical (name, synonym, or CAS number), with scenario notes and Massachusetts contacts; always says occupational limits are not community limits and MRLs are screening levels. |
| `food_access_mapper` | CHR-06 | chronic | USDA Food Access Research Atlas low-income/low-access tracts (four definitions, straight-line or driving) for a town, county, or the state, with people affected, vehicle and SNAP context, HIP and SNAP information, counts of your pantries, SNAP retailers, and HIP vendors by town, a tract map, and a resource list. |
<!-- Phase 14 rows go above this line. -->

### Phase 15: chronic disease prevention and community assessment

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
<!-- Phase 15 screen rows go above this line. -->
| `preventive_services_lookup` | CHR-04 | chronic | USPSTF grade A and B recommendations for a person's age, sex at birth, pregnancy status, and risk factors, each with its grade, statement date, the Task Force's population wording, what to confirm, a plain-language client summary, and a handout option; Massachusetts options (MBCCP, the colorectal program, 1-800-QUIT-NOW) and MyHealthfinder's live list. |
| `asthma_home_assessment` | CHR-05 | chronic | CDC/EPA/HUD home asthma trigger checklist (pointing at the housing code content), DPH's asthma control categories with a classifier, the NHLBI asthma action plan template with peak flow zones, Massachusetts home-based asthma services from DPH's directory, and action plans filled from a local file. |
<!-- Phase 15 prevent rows go above this line. -->
<!-- Phase 15 cha rows go above this line. -->
| `geocode_and_aggregate_addresses` | EPI-13 | epi | Locates the addresses in your own file offline (MassGIS address points and Census TIGER address ranges; an optional full street index, a four-town sample bundled) and returns only counts by town, census tract, or block group with small counts suppressed; reports how many addresses matched at each precision (exact point, interpolated, street only, ZIP only); never returns an address. |
<!-- Phase 15 geo rows go above this line. -->
<!-- Phase 15 rows go above this line. -->

### Phase 16: program planning, evaluation, and evidence

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
<!-- Phase 16 design rows go above this line. -->
<!-- Phase 16 interventions rows go above this line. -->
<!-- Phase 16 appraise rows go above this line. -->
<!-- Phase 16 qi rows go above this line. -->
<!-- Phase 16 rows go above this line. -->

### Phase 17: grants and finance

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
<!-- Phase 17 grants rows go above this line. -->
<!-- Phase 17 budget rows go above this line. -->
<!-- Phase 17 finance rows go above this line. -->
<!-- Phase 17 rows go above this line. -->

### Phase 18: leadership, accreditation, and professional practice

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
<!-- Phase 18 strategy rows go above this line. -->
<!-- Phase 18 workforce rows go above this line. -->
<!-- Phase 18 ethics rows go above this line. -->
<!-- Phase 18 rows go above this line. -->

### Phase 19: policy and law

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
<!-- Phase 19 tracking rows go above this line. -->
<!-- Phase 19 law rows go above this line. -->
<!-- Phase 19 rows go above this line. -->

See [docs/tools.md](docs/tools.md) for arguments, outputs, and privacy behavior of each tool.
For nurses, the [user guide](docs/user-guide/PHN-MCP-Server-User-Guide.docx) (Word) explains
what each tool is for, when to use it, and how to ask for it.
Reference content and data snapshots are also exposed as MCP resources (`phn://content/…`,
`phn://snapshots/…`).

## Quick start

### 1. Install a container runtime

Install [Docker Desktop](https://www.docker.com/products/docker-desktop/) on Windows or macOS.
Docker Desktop needs a paid subscription in larger organizations, which can include state and
municipal agencies; [Podman Desktop](https://podman-desktop.io) and
[Rancher Desktop](https://rancherdesktop.io) are free alternatives. Ask your agency's IT
department which runtime is approved before installing.

### 2. Get the image

Once a release is published:

```sh
docker pull ghcr.io/spiroganas/public-health-nursing-mcp-server:0.15.0
```

On networks that block GHCR, download `phn-mcp-server-v0.15.0-amd64.tar.gz` (or `-arm64` for
Apple silicon) from the GitHub release. Load it, then give the image the name the configuration
below uses:

```sh
docker load -i phn-mcp-server-v0.15.0-amd64.tar.gz
docker tag phn-mcp-server:v0.15.0-amd64 ghcr.io/spiroganas/public-health-nursing-mcp-server:0.15.0
```

(Use `arm64` in both commands on Apple silicon.) Without the second command Docker does not find
the image under that name and tries to download it from GHCR.

To build it yourself from this repository:

```sh
docker build -t ghcr.io/spiroganas/public-health-nursing-mcp-server:0.15.0 .
```

### 3. Create your data folder

Create a folder such as `PHN-Data` in your home folder. Put files you want the assistant to use
in it, and your agency's policies and procedures in `PHN-Data/policies`. Generated documents
appear in `PHN-Data/output`.

### 4. Connect Claude Desktop

Open Claude Desktop → Settings → Developer → Edit Config, and add the server.

**macOS** (`~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "public-health-nursing": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm", "--read-only",
        "-v", "/Users/YOUR-NAME/PHN-Data:/data",
        "-v", "phn-cache:/cache",
        "-e", "PHN_PROFILE=cd_nurse",
        "ghcr.io/spiroganas/public-health-nursing-mcp-server:0.15.0"
      ]
    }
  }
}
```

**Windows** (`%APPDATA%\Claude\claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "public-health-nursing": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm", "--read-only",
        "-v", "C:\\Users\\YOUR-NAME\\PHN-Data:/data",
        "-v", "phn-cache:/cache",
        "-e", "PHN_PROFILE=school_nurse",
        "ghcr.io/spiroganas/public-health-nursing-mcp-server:0.15.0"
      ]
    }
  }
}
```

On **Linux**, add `"--user", "1000:1000"` (your own user and group IDs from `id -u` and
`id -g`) after `--read-only` so the server can write to your folder and the files it creates
belong to you. Docker Desktop on Windows and macOS handles this automatically. The image makes
`/cache` writable for any user, and a new `phn-cache` volume copies that the first time it is
used. A volume first created by version 0.14.0 or earlier is not writable by your user: remove it
once with `docker volume rm phn-cache` (this deletes the saved settings and the download cache,
which are rebuilt) and start the server again.

Restart Claude Desktop. The tools appear under the tools (hammer) icon. Start with:

> Set my jurisdiction to Amherst, and my board of health phone to 413-555-0100.

Then try, for example:

> Is a positive HBsAg reportable, and how fast?
> What does our policy say about needlestick injuries?
> Profile line_list.csv and tell me if it contains identifiers.
> Make a de-identified copy of line_list.csv I can share with the school.

File names are relative to your data folder: say `line_list.csv` or `rosters/kindergarten.xlsx`,
not `PHN-Data/line_list.csv`.

`docker run --rm ghcr.io/spiroganas/public-health-nursing-mcp-server:0.15.0 check` prints the
configuration and verifies the bundled data, which helps when troubleshooting with IT.

## Configuration

Environment variables (set with `-e` in the client configuration) take precedence over the
settings saved by `configure_jurisdiction` in `/cache/phn-settings.toml`.

| Variable | Default | Purpose |
|---|---|---|
| `PHN_PROFILE` | none | Role profile: `school_nurse`, `cd_nurse`, `phn_leader`, or `preparedness`. |
| `PHN_TOOLSETS` | all | Comma-separated toolsets, e.g. `cd,epi`. Overrides the profile. `core` is always on. A value that names no toolset (for example `,`) stops the server with an error. |
| `PHN_SUPPRESSION_THRESHOLD` | `5` | Counts from 1 to threshold − 1 are suppressed (2–30). A threshold outside 2–30 in the saved settings file is ignored (the default is used) and reported at startup and by `configure_jurisdiction`. |
| `PHN_OFFLINE` | off | `1`, `true`, `yes`, or `on` disables all network calls; only bundled and cached data are used. `0`, `false`, `no`, or `off` leaves them on. Any other value stops the server with an error, so a typo cannot leave the network on. |
| `PHN_CA_BUNDLE` | none | PEM file of extra certificate authorities, for agency networks that inspect TLS. Put it in `/cache` or `/data`. |
| `HTTPS_PROXY`, `NO_PROXY` | none | Outbound proxy, if your agency requires one. |
| `PHN_HTTP_TIMEOUT_SECS` | `20` | Timeout for public API calls. |
| `PHN_DATA_DIR`, `PHN_CACHE_DIR`, `PHN_OUTPUT_DIR`, `PHN_POLICY_DIR`, `PHN_SETTINGS_FILE` | `/data`, `/cache`, `/data/output`, `/data/policies`, `/cache/phn-settings.toml` | Folder locations (rarely changed). |
| `PHN_LOG` | `info` | Log level. Logs go to stderr; stdout carries the MCP protocol. |
| `TZ` | `America/New_York` (set in the image) | Time zone for "today" and for dates and times printed on documents. Set another zone with, for example, `-e TZ=America/Chicago`. |
| `PHN_CENSUS_API_KEY`, `PHN_NCBI_API_KEY`, `PHN_OPENFDA_API_KEY`, `PHN_AIRNOW_API_KEY` | none | Optional free API keys. Everything works without them (bundled snapshots or lower rate limits); the Census key is needed for live ACS data from some networks, and the AirNow key adds the AirNow API to the public AirNow files. |

**Profiles and toolsets.** Exposing every tool at once crowds the model's context, so tools are
grouped into toolsets (`core`, `cd`, `vax`, `school`, `mch`, `aging`, `bh`, `epr`, `env`,
`chronic`, `equity`, `epi`, `planning`, `leadership`, `policy`, `comms`, `ebp`). Profiles enable
a sensible set:

| Profile | Toolsets |
|---|---|
| `school_nurse` | core, school, vax, cd, bh, comms, epi |
| `cd_nurse` | core, cd, vax, epi, comms, equity |
| `phn_leader` | core, epi, equity, planning, leadership, policy, comms, ebp, chronic |
| `preparedness` | core, epr, env, epi, equity, comms, cd |

Changing the profile or toolsets with `configure_jurisdiction` refreshes the tool list
immediately in clients that support tool-list notifications. When `PHN_PROFILE` or
`PHN_TOOLSETS` is set in the client configuration (as in the examples above), it fixes the
toolsets: a profile or toolset list saved with `configure_jurisdiction` is kept but has no
effect, and the answer says so. To use a tool outside those toolsets, edit the client
configuration and restart the client.

## Privacy and safety design

- **Sandbox.** Tools read only inside `/data` and write only to `/data/output`; `..`,
  absolute paths outside `/data`, and symbolic links that escape are rejected.
- **Aggregate first.** Data tools return structure, counts, and statistics, never rows, unless
  `include_rows` is set; even then identifier columns are masked, dates reduced to year, ZIP
  codes to three digits, and ages over 89 shown as 90+.
- **PHI detection.** Column names, value patterns, and public name lists (U.S. Census surnames,
  SSA first names) flag the 18 HIPAA identifiers. Document text is redacted before it is
  returned; a document that contains client identifiers is redacted strictly.
- **Small-cell suppression** is applied automatically to every frequency table.
- **Local mail-merge.** Letters and rosters are written to `/data/output` and never returned.
- **No telemetry.** The server makes no outbound calls except to the public data sources listed
  by `get_data_source_status`, and runs fully offline with `PHN_OFFLINE=1` or
  `docker run --network none`.
- **Decision support only.** Clinical and legal answers cite their sources and say how current
  they are. No tool submits anything to an external system.

Automated detection can miss identifiers. Review outputs before sharing them, and follow your
agency's policy on using AI tools.

## Development

Requirements: Rust 1.88 or later (stable). Then:

```sh
cargo test --workspace                 # unit, golden, and MCP integration tests
cargo +stable clippy --workspace --all-targets -- -D warnings   # CI uses the latest stable clippy
cargo run -p phn-server --bin phn-mcp-server -- check
cargo xtask validate-content           # schemas, review dates, snapshot checksums
cargo xtask build-snapshots --check    # rebuild MassGIS snapshots and diff them
cargo xtask build-name-lists           # rebuild the PHI detector's name lists
```

The [MCP Inspector](https://github.com/modelcontextprotocol/inspector) is handy for manual
testing: `npx @modelcontextprotocol/inspector cargo run -q -p phn-server --bin phn-mcp-server`
(set `PHN_DATA_DIR` and `PHN_CACHE_DIR` to local folders first).

Repository layout:

```
crates/
  phn-server   # binary: rmcp stdio server, toolset filtering, MCP prompts and resources
  phn-core     # configuration, errors, response envelope, toolsets
  phn-data     # HTTP client, SQLite cache, snapshot store, Massachusetts reference lists,
               # adapters for public data sources (live → cache → snapshot)
  phn-content  # reference content (YAML + JSON Schema); every file is pending review
  phn-epi      # rates and intervals, suppression, standardization, 2×2 tables, regression,
               # trends, hypothesis tests, SPC, aberration detection, queueing, survey estimates
  phn-cdsi     # CDC CDSi supporting data and the immunization evaluation and forecasting engine
  phn-local    # /data sandbox, parsers, PHI detection, de-identification, policy index
  phn-docs     # DOCX/PDF/XLSX/CSV/SVG/PNG output, mail-merge, accessibility checks, charts
  phn-tools    # tool implementations, one folder per theme
content/       # reference content with citations and review metadata
snapshots/     # dated data snapshots + manifest (built by xtask)
fixtures/      # synthetic test corpus (all fictional)
docs/          # tool reference, user guide (Word), architecture decisions, acceptance scripts,
               # CDSi test results
xtask/         # snapshot builders (Rust, and Python in scripts/), name-list builder,
               # content validator
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to add a tool and how content review works, and
[CHANGELOG.md](CHANGELOG.md) for release notes. [STATUS.md](STATUS.md) is the handoff document
(what is done, what is open, lessons learned), and [CLAUDE.md](CLAUDE.md) holds the standing
instructions for AI coding agents.

## License

MIT. Bundled DejaVu fonts are under the Bitstream Vera license (see
`crates/phn-docs/fonts/LICENSE_DEJAVU`). MassGIS data are public records of the Commonwealth of
Massachusetts. Census and SSA name lists are public domain.
