# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/). Version numbers track the phases in
[`project_plan.md`](project_plan.md).

No version has been tagged or published as a release yet. The date on each version is the day
it was merged to `main` (Eastern time).

## [Unreleased]

## [0.15.0] — 2026-11-11 (not yet tagged) — Phase 14: environmental health and climate

### Added

- Environmental health tools (toolset `env`):
  - `drinking_water_quality_lookup` (ENV-03): the public water systems for a town from EPA
    SDWIS (live through Envirofacts, with a bundled snapshot): health-based violations, other
    violations by category, the latest lead and copper 90th percentiles against the action
    levels, MassDEP PFAS6 results for treated water (EEA Data Portal), what residents can do,
    the Massachusetts and federal PFAS and lead standards, and private well testing guidance
    when community systems do not serve most of the town.
  - `environmental_justice_screen` (ENV-04): Massachusetts 2020 environmental justice block
    groups for a town, county, or the state, by criterion and combination, with the people
    living in them (EEA's municipal statistics for towns), English-isolation block groups and
    the largest limited-English language groups, SVI context, what designation changes (MEPA,
    translation and interpretation, EEA policy, DPH's vulnerable health criteria), an optional
    map and table, and an optional summary of an archived EJScreen 2.3 file saved in `/data`
    (averages and counts only).
  - `climate_health_vulnerability_profile` (ENV-05): a town profile for MVP and hazard
    mitigation planning: extreme heat days, tree canopy, and impervious surface (CDC/ATSDR Heat
    and Health Index), NFIP flood claims and Community Rating System status, social
    vulnerability and climate-sensitive health, MVP region and plan status, FEMA hazard
    mitigation plan status (live from OpenFEMA, with a snapshot), and the most important
    vulnerabilities by labeled tool-default rules, with a Word option.
  - `vector_borne_disease_risk` (ENV-06): DPH's EEE and West Nile virus risk levels by town
    (live in season; otherwise the dated last levels, clearly not current risk) with DPH's
    response for each level, positive mosquito and animal findings, tick-borne disease cases
    and rates by county (2022–2025) and emergency department visits, and prevention messages
    for the public, schools, camps, and outdoor workers, with an optional one-page notice.
  - `housing_conditions_health_guide` (ENV-07): the State Sanitary Code (105 CMR 410.000) for
    moisture and mold, pests, heat, hot water and utilities, lead, smoke and CO alarms, and
    trash: code sections, owner and occupant duties, conditions always deemed to endanger
    health, health effects, the inspection process and timelines, tenant protections (with a
    not-legal-advice notice), tenant teaching sheets at or below grade 8 (agency translations
    only), and inspection and repair request letters filled locally into `/data/output`.
  - `chemical_exposure_reference` (ENV-08): the NIOSH Pocket Guide entry and current IDLH,
    ATSDR minimal risk levels, and ToxFAQs links for a chemical by name, synonym, or CAS number
    (ambiguous names are refused with the candidates), with notes and contacts for spills,
    indoor air, contaminated sites, drinking water, and workplaces. Every answer starts by
    saying that occupational limits are not community limits and that MRLs are screening
    levels.
- Chronic disease tools (toolset `chronic`):
  - `food_access_mapper` (CHR-06): USDA Food Access Research Atlas low-income, low-access
    tracts under all four definitions (the 2025 map, which counts any SNAP store, and the 2019
    supermarket map carried to 2020 tracts) for a town, county, or the state, with the people
    affected, vehicle and SNAP context, HIP and SNAP facts, counts by town from your pantry,
    SNAP retailer, and HIP vendor files, a tract map, a Word resource list, and an Excel
    workbook. Site names and addresses go only into the output files.
  - `community_water_fluoridation_status` (CHR-07): a town's fluoridation status from DPH's
    2023 list and CDC's 2025 My Water's Fluoride report with the systems behind it, a statewide
    summary, the standards, each body's current position and the recent evidence reviews side
    by side with their dates, M.G.L. c. 111, § 8C in plain language, and an optional board of
    health brief.
- Content drafted from EPA, MassDEP, DPH, EEA, MEPA, CDC, ATSDR, NIOSH, FEMA, USDA, and DTA
  sources and the statutes and regulations they cite, all pending review:
  `content/env/drinking_water.yaml`, `environmental_justice.yaml`, `climate_vulnerability.yaml`,
  `vector_borne.yaml`, `housing_conditions.yaml`, `chemical_exposure.yaml`, and
  `content/chronic/fluoridation.yaml` and `food_access.yaml`, with schemas.
- Snapshots, each with a builder in `xtask/scripts/` that has a `--check` mode: EPA SDWIS and
  MassDEP PFAS6, CDC My Water's Fluoride and DPH's fluoridation list, the 2020 EJ block groups
  and town statistics, the Heat and Health Index, NFIP claims and community status, FEMA hazard
  mitigation plans, MVP communities, DPH arbovirus risk levels and positives, DPH tick-borne
  disease cases and ED visits, the NIOSH Pocket Guide and IDLH table, ATSDR MRLs and ToxFAQs,
  and the USDA Food Access Research Atlas.
- Synthetic fixtures with seeded identifiers (an EJScreen extract, housing requests, a pantry
  list); every new data tool is in the leak test.
- Acceptance script `docs/acceptance/phase-14.md`; user guide chapter 17 and a rebuilt guide
  for 110 tools (135 pages).

### Fixed

- `heat_health_action_planner` (and the new climate profile): the population table's "about how
  many people" multiplied every percent by the total population, so a town could show more adults 65+ with a disability than adults 65+. Each count
  now uses the indicator's own base (adults 65+, people age 5+, or households), and tables show
  the unit when it is not people.

## [0.14.1] — 2026-11-11 (not yet tagged) — Peer review of Phases 0–13

A review of the code and documentation for Phases 0–13 found errors in calculations, places where
private information could reach the AI model, and documentation that no longer matched the code.
This release fixes them. No tools were added or removed; `docs/tools.md` describes each tool's
new behavior. ADR 0010 records the decisions made in the review.

### Security and privacy

- Local files: headings and titles are now redacted like the rest of the text in `read_local_file`
  and `search_agency_policies`, and documents that hold a client table or roster (in Word tables
  or tab-, pipe-, or semicolon-separated text) are redacted strictly.
- Local files: column headers and sheet names that look like names or values (a file with no
  header row) are shown as `column_N` and `sheet_N`; more name and ID columns are recognized
  ("First", "Last", "Student", "Guardian", "ID", "Case #"); more date, phone, SSN, and record
  number formats are detected.
- `read_local_file` sample rows: ZIP codes use the Safe Harbor rule (000 for low-population
  prefixes), a birth year that shows an age of 90 or more is `90+`, and long numbers in unknown
  columns are masked.
- `profile_local_dataset`: no statistics for a column with fewer values than the suppression
  threshold; ages over 89 top-coded before any statistic; smallest and largest values hidden for
  ID-like columns.
- `apply_small_cell_suppression`: numeric label columns (grade, age, week, ZIP) are no longer
  treated as counts, which could undo the suppression; label cells with names, phone numbers, or
  free text are refused; error messages no longer quote cell values.
- `deidentify_dataset`: Safe Harbor removes dates and ZIP codes inside text cells; a limited
  data set drops latitude and longitude.
- `check_document_accessibility`: quoted excerpts are redacted, and the privacy block reports it.
  `export_document` mail merges no longer return merged text in the accessibility messages.
- Small counts that could be worked out by subtraction are now hidden in
  `track_school_screenings`, `build_school_health_annual_report`, `screen_health_related_social_needs`
  (by group), `score_mental_health_screen` (file mode), `plan_casper_assessment`, and
  `create_spc_chart` (center line and limits withheld); `analyze_outbreak_line_list` withholds the
  median onset and exposure window in very small outbreaks; `create_chart` hides a suppressed
  value's confidence limits.
- Language cells are never returned: unrecognized languages are reported as "Other" in school
  letters and health plans, and letter files are never overwritten.
- Staff contacts (ADR 0010): a name not found on the roster gets no contact details, instead of
  those of the first person in the same position (decided by the product owner on 2026-11-11).
  Typed-in phone numbers and emails are now detected in every field of the Phase 13 tools.
- A saved suppression threshold outside 2–30 (for example after a hand edit) is ignored instead of
  switching suppression off.
- CSV output marks cells that start like a spreadsheet formula as text.
- Network: requests send no Referer header; openFDA's API key is no longer stored in the cache;
  the NWS forecast link is followed only to api.weather.gov; an error page from a data source is
  no longer cached as data.
- Release workflow: the image is scanned before anything is pushed, the SBOM lists the Rust
  crates (built from the source tree), and the workflow runs only for version tags.

### Fixed

- Statistics: rank tests (Mann–Whitney, Wilcoxon) gave p = 0 when there was no difference at all;
  they now give p = 1. t tests and ANOVA on data with no variation give p = 1 instead of an
  undefined value.
- Statistics: chi-square degrees of freedom and Cramér's V ignore empty rows and columns;
  Breslow–Day degrees of freedom ignore empty strata.
- Statistics: logistic regression in `run_statistical_test` warns about separation and models that
  did not converge, instead of reporting huge odds ratios as "not significant".
- Statistics: SPC trend and shift rules no longer count repeated values or points on the center
  line (a flat series is never a trend).
- Statistics: an all-zero count series is refused for rate and trend models; a correlation with a
  constant variable is refused; Fisher's exact test falls back to chi-square when it cannot be
  calculated; very large 2×2 tables skip the exact methods instead of running for minutes.
- `analyze_trends`: seasonal adjustment is on by default only with at least two years of monthly
  or weekly data; the seasonal peak is placed correctly for labels such as "Mar 2024".
- `age_adjust_rates`: warns when age groups do not cover all ages or differ between groups.
- Dates (every tool that reads dates from files, and the school, immunization, growth, and
  communicable disease tools): "3/5/26" was read as the year 26; two-digit years are now read in
  the current century unless that would be more than a year in the future (then the previous
  century), and years outside 1900–2100 are refused instead of crashing date arithmetic.
- Disease and pathogen lookups no longer guess: `lookup_reportable_condition`,
  `calculate_isolation_and_exclusion`, and `infection_control_for_congregate_settings` had
  answered "hepatitis" as hepatitis A, "gastroenteritis" as group A strep, "stomach flu" as
  influenza, and "ebola" as MRSA. Ambiguous or general queries now list the candidates.
- `plan_contact_tracing`: workbook columns now say which contacts each exclusion applies to
  (pertussis and hepatitis A rules had read as applying to every contact).
- `assess_measles_exposure`: an infant under 12 months with reported doses was treated as immune;
  PEP deadlines now count only exposure inside the case's infectious period.
- `calculate_isolation_and_exclusion`: "still excluded on" checks work for rules counted in hours.
- `plan_tb_contact_investigation`: a symptom onset date now sets the infectious period even when
  "symptomatic" was not marked; ages such as "8 months" are read.
- `analyze_outbreak_line_list`: an unknown agent is a caveat, not an error; exposures that could
  not be compared are listed; a "Meal Time" column is no longer taken for the onset time.
- `get_notifiable_disease_trends`: the "last 4 weeks" baseline was 3 weeks long for week 53.
- `check_ma_school_immunization_requirements`: students 7 and older with DTaP gaps were told
  "not met" and offered DTaP (now "needs review"); doses too soon after an invalid dose, numbered
  roster columns ("MMR 1"), and Exemptions cells such as "Religious" are handled.
- `forecast_immunizations_due` (file mode): doses dated before birth or after the assessment date
  are counted and listed instead of being silently treated as evaluated.
- `vaccine_temperature_excursion_helper`: Fahrenheit readings such as 46.3 °F were judged in range.
- `lookup_medication_information`: shortages are searched by generic name (a brand name had
  returned "none listed"); a close but inexact RxNorm match now carries a caveat.
- `score_mental_health_screen`: a C-SSRS screener with unanswered deciding items was scored low
  risk (now "risk level not yet determined"); unanswered suicide-risk items are flagged.
- `find_substance_use_treatment`: "Medicaid" and "Spanish" filters matched the wrong SAMHSA values;
  out-of-state facilities are flagged.
- `nursing_home_quality_lookup` and `find_community_resources`: facilities listed under a village
  or neighborhood (Hyannis, Dorchester, Middleboro) were missed; they are matched by ZIP code.
- `elder_protective_services_guide`: whether a role is a mandated reporter now follows the list for
  the chosen pathway (c.19A or c.19C).
- `steadi_fall_risk_assessment`: a count of falls now counts as a fall in the past year.
- School tools: "Negative" results count as passed; heights such as 4'10" are no longer read as
  410; "Speech/Language Services" is no longer taken for the language column; a student with two
  conditions gets both health plans; a "yes" notice without a date is not a gap; the annual report
  in July or August covers the school year that just ended; visit reasons no longer match inside
  words ("distress" is not "stress").
- `check_developmental_milestones`: "not yet" on the corrected-age checklist now counts.
- `get_active_hazard_alerts` and `heat_health_action_planner`: a stale cached copy of the NWS feed
  was shown as current alerts, and a failed feed read as "no alerts"; ended alerts are dropped.
- `heat_health_action_planner`: a HeatRisk image from an earlier forecast is no longer paired with
  new dates; the peak day is the first day of the highest level.
- `air_quality_protective_guidance`: concentrations above the 8-hour ozone and 1-hour SO2 tables
  are refused instead of extrapolated.
- `build_community_health_profile`: a top-coded median income showed as $250,001.
- `translate_health_material`: numbers such as 3.5, 1,000, and 9:30 were reported as missing;
  glossary terms match whole words and plurals.
- `check_readability`: syllable counting fixed for many common words, such as safety, serious,
  diarrhea, and influenza (grades can shift a little); `draft_press_materials` no longer flags
  "Hispanic" as containing "panic".
- `omaha_system_coder`: negated problems ("no income") are listed for review instead of dropped.
- `plan_casper_assessment`: the sampling interval accounts for blocks selected twice; analyzing
  returned data without a seed no longer draws a new sample.
- `generate_ics_forms`: the ICS 207 shows branches; an incident name with a slash no longer
  changes the output folder.
- `continuity_of_operations_planner`: staff at the peak were undercounted (10 staff at 80% absence
  gave 1); a position held by several people no longer makes someone their own successor.
- `design_hseep_exercise`: warns when the schedule is longer than the exercise; MSEL events are
  numbered within each module.
- `build_after_action_report`: ratings naming "PHEP 13: …" were dropped silently; the objectives
  from `design_hseep_exercise` can be passed unchanged.
- `crisis_risk_communication_builder`: an agency translation was used even after the English
  message was changed; "space heater fire" no longer brings up the heat messages; bracketed
  placeholders no longer pass the checks.
- `export_document`: a `---` line no longer starts a new page; nested lists and indented text are
  kept in order; words no longer run together in PDFs; repeated column names and long sheet
  names no longer break XLSX files; text such as "1,2" stays text.
- `create_chart` refuses a log scale for bar charts; charts with a very narrow range no longer hang.
- Crashes fixed for file names, sheet names, and dates with accented or very long text.
- `search_pubmed`: an NCBI error is reported instead of being cached as "no results".

### Changed

- The Docker image runs on Eastern time (`TZ=America/New_York`), so "today" and document dates no
  longer roll over at 7 or 8 p.m. (ADR 0010). `phn-mcp-server check` prints the local time.
- The Docker image makes `/cache` writable for Linux users who run it with `--user`. A
  `phn-cache` volume created by an earlier version must be removed once (see the README).
- `PHN_OFFLINE` accepts only 1/true/yes/on or 0/false/no/off, and `PHN_TOOLSETS` must name at
  least one toolset; anything else stops the server with an error.
- When `PHN_PROFILE` or `PHN_TOOLSETS` is set in the client configuration, `configure_jurisdiction`
  says that saved toolsets have no effect, and a disabled tool's error says to edit the
  configuration.
- `functional_needs_shelter_intake` uses the same translation folder names as the LTBI calendar
  and newcomer checklist (`haitian_creole`), and is marked as using the internet.
- The lead and newborn teaching sheets also accept a `haitian-creole` folder.
- The `investigate_outbreak` and `plan_tabletop_exercise` prompts name the toolsets they use and
  say what to do when one is off; the outbreak prompt no longer suggests "GI illness, agent
  unknown" as a disease name.
- MCP resources: content descriptions say "pending review" instead of "Reviewed reference
  content"; JSON and text snapshots have the right MIME type.
- Cached public data older than 90 days is deleted; a damaged cache is skipped instead of
  stopping a lookup; retrieval dates are local dates.
- Documentation: README (offline image install, repository layout, configuration), CONTRIBUTING,
  `project_plan.md` §5 (the architecture as built), the user guide's profile table, the acceptance
  scripts, and new ADR 0010 (superseding notes for ADRs 0001, 0004, and 0008; time zone; roster
  name matching).

## [0.14.0] — 2026-11-09 (not tagged) — Phase 13: emergency preparedness II (ICS forms, COOP, HSEEP exercises, after-action reports, CERC)

### Added

- Preparedness document tools (toolset `epr`):
  - `generate_ics_forms` (EPR-01): ICS 201, 202, 203, 204, 205, 205A, 206, 207, 208, 213, 213RR,
    214, and 215A in Word and/or PDF, with FEMA's titles, block numbers, and labels (checked
    against FEMA's v3/v3.1 forms), an assembled Incident Action Plan, blank forms with FEMA's block
    instructions, the required blocks still blank, and suggested objectives and safety lines for
    seven public health incident types.
  - `continuity_of_operations_planner` (EPR-07): a board of health continuity of operations plan
    by FEMA's ten continuity elements, with 19 Massachusetts essential functions (authority,
    priority tier, recovery time, minimum staff), orders of succession, delegations of authority,
    continuity facilities, communications, essential records, a 40% absenteeism staffing check,
    a first-year test, training, and exercise schedule, devolution, reconstitution, and a pandemic
    annex (Word plan and Excel workbook).
  - `design_hseep_exercise` (EPR-08): HSEEP exercise design for all seven exercise types, with a
    SMART check of each objective, mapping to the 15 CDC PHEP capabilities and FEMA core
    capabilities, fully developed tabletop scenarios for a measles outbreak, a coastal storm with
    a power outage, and extreme heat (Pilot 3), scenario seeds for six other hazards, and the
    Situation Manual, Facilitator Guide, Exercise Plan, MSEL, Exercise Evaluation Guides, feedback
    form, and sign-in sheet.
  - `build_after_action_report` (EPR-09): the HSEEP after-action report and improvement plan for
    an exercise or a real incident, with corrective-action, non-attribution, and client-identifier
    checks and participant feedback-form totals computed locally with small-cell suppression.
  - `crisis_risk_communication_builder` (EPR-10): CDC CERC message maps checked against the
    27-word, 9-second, 3-message rules, holding statements, FAQs, spokesperson preparation, and
    phase tasks for seven pre-scripted hazards or your own, with the town's priority languages,
    agency translations from `/data/templates/<language>/`, and translator briefs (no machine
    translation).
- MCP prompt `plan_tabletop_exercise`: an HSEEP tabletop exercise cycle (hazard context, design,
  player materials, hotwash, after-action report) using the Phase 12 and 13 tools.
- `epr::roster` and ADR 0009: staff names and positions may be passed in a request; phone
  numbers, emails, and radio call signs come only from a roster file in `/data` and are written
  only to documents in `/data/output`. Contact details found in a request are reported in the
  privacy block.
- Content drafted from FEMA's ICS forms, HSEEP 2020 and its templates, CDC's 2018 PHEP
  capability standards, the CERC manual and message-mapping guides, M.G.L. c. 111 and c. 114,
  and CDC and DPH hazard pages, all pending review: `content/epr/ics_forms.yaml`,
  `continuity.yaml`, `capabilities.yaml`, `hseep.yaml`, `exercise_scenarios.yaml`, and
  `cerc.yaml`, with schemas.
- Synthetic fixtures: a fictional staff roster and exercise feedback forms with seeded
  identifiers; every new tool is in the leak test.
- Acceptance script `docs/acceptance/phase-13.md`; user guide rebuilt for 102 tools (124 pages).

### Changed

- The tool registry lists phases as (phase, tools) pairs so a phase can have several
  work-package lists.
- COMM-02 exposes its language-priority and translator-brief helpers, and COMM-04 shares its
  loaded-word check, for reuse by the CERC builder.

## [0.13.0] — 2026-11-08 (not tagged) — Phases 7 to 12: school health, maternal and child health, PHN practice, healthy aging, substance use and communication, and emergency preparedness I

### Added

- User guide for public health nurses (`docs/user-guide/PHN-MCP-Server-User-Guide.docx`): an
  executive summary of the server's benefits, how it works and protects privacy, getting
  started, and for each implemented tool what it does, when to use it, how to ask for it, what
  it returns, and what to keep in mind; plus a tool index, glossary, and troubleshooting. The
  guide was first written after Phase 6, for 47 tools, and was rebuilt for this version's 97
  tools (116 pages). Built from `docs/user-guide/guide-content.js` by `build-user-guide.js`; a
  registry test keeps the guide's tool list in step with the implemented tools.

### Added — Phase 7: school health

- School health tools (toolset `school`): `get_ma_school_screening_requirements` (SCH-01),
  `track_school_screenings` (SCH-02), `school_medication_administration_guide` (SCH-03),
  `build_individualized_health_plan` (SCH-04), `concussion_return_to_learn_tracker` (SCH-05),
  `monitor_school_absenteeism` (SCH-06), `build_school_health_annual_report` (SCH-07), and
  `child_care_health_consultation_checklist` (SCH-08).
- Content drafted from 105 CMR 200.000, 201.000, 210.000 (12/5/2025), 300.000, 606 CMR 7.00,
  M.G.L. c. 71 sections 53-57, 54B, and 97, c. 111 section 222, DPH school health protocols and
  guidance, and CDC pages, all pending review: `content/school/*.yaml` with schemas, and the CDC
  BMI-for-age LMS reference copied from `bmiagerev.csv`.
- School-year (July-June) and grade logic, BMI-for-age percentiles (golden test against CDC's
  published percentiles), and letter and plan templates with local fill and agency translations
  in `/data/templates/<language>/`.
- Fictional fixtures (screening roster, absence log, health office visits, head injury log,
  health plan students) with a seeded generator, added to the leak test.

### Added — Phase 8: maternal, child, and family health

- `calculate_growth_percentiles` (MCH-03): WHO 2006 (birth to 24 months) and CDC 2000 (2 to 20
  years) percentiles and z-scores with CDC's 2022 extended BMI-for-age method, BMI categories,
  corrected age for prematurity, extreme-value flags, visit-to-visit change, and a batch mode
  that writes per-row results to the output folder. Snapshots `who-growth-standards` and
  `cdc-growth-charts` (`xtask/scripts/build_growth_charts.py`); content
  `content/mch/growth_interpretation.yaml` (pending review); golden tests against CDC's and
  WHO's published tables.
- `childhood_lead_screening_guide` (MCH-04): screening schedule and high-risk schedules from
  105 CMR 460.050, DPH's 2024 high-risk community list with each town's screening rate, venous
  confirmation and follow-up by blood lead level from DPH CLPPP, reporting, the Lead Law, family
  teaching, and an optional teaching sheet (English or an agency translation in
  `/data/templates/<language>/`). Content `content/mch/lead_screening.yaml` (pending review);
  snapshot `ma-lead-screening` from DPH's 2024 surveillance report
  (`xtask/scripts/build_lead_snapshot.py`).
- `mandated_reporter_51a_guide` (MCH-07): M.G.L. c. 119, s. 51A guidance (who, when, how,
  contents, penalties and protections, DCF definitions, domestic violence, DCF process, warning
  signs), the 48-hour written-report deadline, and the written-report narrative template; with
  `notes_path` it fills a draft from the nurse's local notes into `/data/output` and returns
  only file names and field counts. Content `content/mch/mandated_reporting.yaml` (pending
  review).
- `check_developmental_milestones` (MCH-02): CDC "Learn the Signs. Act Early." checklists
  (2 months to 5 years) with milestone IDs, act-early steps, AAP screening ages, the
  corrected-age checklist for preterm infants, Massachusetts Early Intervention referral, and
  the EI programs serving a town. Content `content/mch/developmental_milestones.yaml` (pending
  review); snapshot `ma-ei-programs` (DPH's EI program directory).
- `plan_newborn_postpartum_visit` (MCH-01): newborn and postpartum home visit plan (feeding and
  weight change, jaundice, AAP 2022 safe sleep, Massachusetts car seat law, newborn blood spot
  and hearing screening under 105 CMR 270.000 and M.G.L. c. 111, s. 67F, CDC Hear Her urgent
  maternal warning signs) with an optional DOCX visit plan and family teaching sheet. Content
  `content/mch/newborn_postpartum.yaml` (pending review; jaundice items low confidence).
- `estimate_benefit_eligibility` (EQ-06): income screen for WIC, SNAP, MassHealth, the Health
  Safety Net, and HEAP with the 2026 poverty guidelines and each program's current limits and
  effective dates. Content `content/equity/benefit_eligibility.yaml` (pending review; refresh
  annually).
- `map_lead_risk` (MCH-05): outreach ranking of towns or tracts (mean percentile rank of
  pre-1950/1980 housing, low income, young children, DPH screening gap and elevated blood lead
  prevalence) with DPH's high-risk designation and a choropleth map. Snapshot
  `ma-acs5-housing-age` (ACS B25034, `xtask/scripts/build_housing_age.py`).
- `maternal_infant_health_profile` (MCH-06): community births, low birthweight, preterm, and
  teen births with exact intervals and state/U.S. comparisons; statewide indicators and infant
  mortality by race and Hispanic ethnicity; PRAMS indicators; trends. Snapshot
  `ma-maternal-infant` (DPH birth, death, and PRAMS reports; CDC NCHS provisional estimates;
  `xtask/scripts/build_maternal_infant.py`).
### Added — Phase 9: PHN practice toolkit and case management

- `plan_tb_contact_investigation` (CD-11) and `ltbi_treatment_calendar` (CD-12): the 2005
  NTCA/CDC contact investigation rules (infectious period, Figure 2-4 priorities, Table 3 time
  frames, 8-10 week retest) with an XLSX tracking workbook and local contact-list summaries, and
  LTBI dose calendars, completion windows, and adherence checks for 3HP, 4R, 3HR, 6H, and 9H with
  an XLSX tracker and DOCX patient calendar (no dose calculation).
- `screen_health_related_social_needs` (EQ-05): CMS AHC HRSN scoring by domain with Massachusetts
  referral categories and a batch mode with suppressed counts.
- `omaha_system_coder` (CORE-01), `classify_intervention_wheel` (CORE-02), and
  `nursing_documentation_template` (CORE-03): Omaha System coding, Public Health Intervention
  Wheel classification with workload counts, and SOAP/DAR/narrative templates with a
  required-elements check; notes are analyzed locally and never returned.
- `find_community_resources` (EQ-07): the nurse's resource directory plus HRSA health center sites
  (new `phn_data::hrsa` adapter and `hrsa-health-centers-ma` snapshot with its builder script).
- `newcomer_health_navigator` (EQ-11): CDC domestic refugee screening, DPH/ORI programs, MassHealth
  coverage by immigration status including the October 2026 federal changes, and EA shelter
  resources.
- Content (pending review): `content/cd/tuberculosis.yaml`, `content/equity/ahc_hrsn.yaml`,
  `content/equity/newcomer_health.yaml`, `content/core/omaha_system.yaml`,
  `content/core/intervention_wheel.yaml`, `content/core/documentation_templates.yaml`; agency
  translations are read from `/data/templates/<language>/`; fictional TB contact, HRSN screening,
  and resource directory fixtures.
### Added — Phase 10: healthy aging and mental health screening

- `score_mental_health_screen` (MH-02): scoring engine for PHQ-2, PHQ-9, GAD-2, GAD-7, EPDS, the
  NIMH ASQ, and the C-SSRS screener with range checks, published cutoffs, safety-alert rules and
  cited steps, license terms per instrument, file mode with suppressed counts, and 50 golden cases.
- `find_crisis_resources` (MH-01): 988, the Behavioral Health Help Line, mobile crisis, the Crisis
  Text Line, and Community Behavioral Health Centers by town, with language access.
- `steadi_fall_risk_assessment` (AGE-01), `elder_protective_services_guide` (AGE-03),
  `find_ma_aging_services` (AGE-02), `explain_older_adult_coverage_options` (AGE-04),
  `nursing_home_quality_lookup` (AGE-06), `map_older_adult_vulnerability` (AGE-05), and
  `advance_care_planning_toolkit` (AGE-07).
- Town-to-service directory framework and snapshots (ASAPs, SHINE offices, senior nutrition
  projects, CBHCs) with `cargo xtask build-service-directories`; CMS Provider Data Catalog adapter
  (`phn_data::cms_provider`) with `cargo xtask build-nursing-home-snapshot`; older-adult ACS
  indicators (`phn_data::older_adults`) with `cargo xtask build-older-adult-snapshots`.
- Template + local fill helper (`phn_tools::aging::localfill`) with agency translations from
  `/data/templates/<language>/` and English fallback counted by language.
- Content (all pending review): `content/bh/mental_health_screening.yaml`,
  `content/bh/crisis_resources.yaml`, and `content/aging/*.yaml` (STEADI, protective services,
  aging services, coverage options, nursing home guide, advance care planning).
### Added — Phase 11: substance use, crisis resources, and health communication

- `check_readability` (COMM-01): reading-level formulas (Flesch Reading Ease, Flesch-Kincaid,
  SMOG with CDC's conversion table, Gunning Fog), plain-language flags with CDC Everyday Words
  alternatives, and the CDC Clear Communication Index scored from the nurse's answers. The text
  statistics moved to `phn_tools::comms::textstats` (VAX-09 now uses them) with better sentence
  splitting and syllable counting. Content: `content/comms/readability.yaml`. `comms.rs` became
  the `comms/` folder.
- `get_overdose_data` (MH-03): DPH town-level opioid-related overdose deaths (2015-2022 table,
  bundled), 5-year rates with exact intervals compared with the state, CDC/NCHS provisional county,
  state, and U.S. counts by drug class (live → cache → snapshot), and DPH's 2024 findings.
  Snapshots `ma-dph-opioid-deaths-town` and `cdc-vsrr-overdose-ma` (builders in `xtask/scripts/`);
  content `content/bh/overdose_data.yaml`.
- `get_youth_risk_behavior_data` (MH-07): DPH Youth Health Survey (2023, 2025) and Massachusetts
  YRBS (2023) report tables by sex, grade, race/ethnicity, and LGBTQ identity with 95% CIs and
  differences by non-overlapping intervals, and CDC YRBS mental health indicators for
  Massachusetts vs the U.S. Snapshots `ma-youth-health-survey` and `cdc-yrbs-mental-health-ma-us`;
  content `content/bh/youth_survey.yaml` (also holds DPH's 988 Lifeline text).
- `find_naloxone_and_harm_reduction` (MH-04): naloxone access (statewide standing order of July
  13, 2026, pharmacy policy, programs), syringe service program approval by town, fentanyl test
  strip facts, help lines, and a cited teaching script with an optional Word handout. Content
  `content/bh/substance_use_resources.yaml` (shared with MH-05).
- `find_substance_use_treatment` (MH-05): SAMHSA FindTreatment.gov adapter
  (`phn_data::findtreatment`, live → cache → bundled statewide snapshot `samhsa-findtreatment-ma`)
  with MOUD, setting, special-group, payment, and language filters, and the Massachusetts
  Substance Use Helpline.
- `plan_school_postvention` (MH-06): After a Suicide toolkit steps and talking points (with
  pages), message templates by cause-of-death status filled locally into Word files, and a
  safe-messaging check of drafts. Content `content/bh/postvention.yaml` and
  `content/comms/safe_messaging.yaml` (shared rules, `phn_tools::comms::safe_messaging`); fixture
  `fixtures/synthetic/data/postvention_details.csv`.
- `translate_health_material` (COMM-02): translation workflow with language priorities from EQ-03,
  agency translations in `/data/templates/<language>/`, a translator brief, glossary and
  back-translation checks, the qualified-translator draft label, and an XLSX review worksheet.
  Content `content/comms/translation.yaml`.
- `create_social_media_campaign` (COMM-03) and `draft_press_materials` (COMM-04): campaign
  calendars tied to observances with post checks and an XLSX export; press materials built and
  checked against CDC CERC media guidance with a Word export. Content
  `content/comms/campaigns.yaml` (platform limits and observance dates marked unverified).
### Added — Phase 12: emergency preparedness I: situational awareness and planning

- `get_active_hazard_alerts` (EPR-05): live NWS alerts for a town, county, or the state with
  cited health effects, at-risk groups, public health actions, and messages, and OpenFEMA
  disaster declarations (snapshot fallback).
- `model_dispensing_site_throughput` (EPR-02): M/M/c (Erlang C) model of POD and mass
  vaccination stations with sizing, waits, bottleneck, capacity, hours to reach a population,
  and an optional staffing workbook; `phn_epi::queueing` with golden tests.
- `air_quality_protective_guidance` (ENV-02): AirNow current and forecast AQI by reporting area
  with EPA and CDC protective actions, and 2024 Appendix G concentration-to-AQI conversion.
- `heat_health_action_planner` (ENV-01): NWS HeatRisk and forecast, heat-vulnerable population
  (ACS, PLACES, SVI, emPOWER), activation steps, and a DOCX heat response checklist.
- `hazard_vulnerability_assessment` (EPR-06): facilitated HVA seeded with ResilientMass 2023,
  FEMA declarations, and optional NRI tables, with an XLSX workbook.
- `functional_needs_shelter_intake` (EPR-03): CMIST intake template and blank form, local fill
  with agency translations, and shelter planning estimates.
- `get_electricity_dependent_population` (EPR-04): HHS emPOWER counts by town, county, and ZIP
  code from the emPOWER Map's JSON endpoints (snapshot fallback).
- `plan_casper_assessment` (EPR-11): CASPER cluster selection from 2020 census blocks with a
  reproducible seed, team materials, and weighted analysis (`phn_epi::survey`, golden test
  against samplics).
- Data adapters (`phn_data::nws`, `openfema`, `empower`, `airnow`, `heatrisk`,
  `census_blocks`, `projection`) and snapshots (FEMA declarations, emPOWER, 2020 census block
  housing units) built by `xtask/scripts/build_epr_snapshots.py`; eight content files in
  `content/epr/` and `content/env/` (pending review).
### Changed and fixed during integration

- The communication tools moved from `comms.rs` to `crates/phn-tools/src/comms/`; COMM-05 now
  lives in `comms/accessibility.rs`, and VAX-09's readability check uses the shared COMM-01 engine.
- Two user guide entries used an internet value the Word builder did not know, which broke
  `build-user-guide.js`; a registry test now checks every entry's value.
- The leak test also covers `create_social_media_campaign` pointed at an identifiable file.
- The data source registry lists every Phase 7–12 tool that reads local files or ACS data.
- `.gitignore` (a Python template) ignored every `env/` folder; exceptions keep
  `crates/phn-tools/src/env/` and `content/env/` in the repository.
- The former MOLST domain (molst-ma.org) no longer belongs to the program; the advance care
  planning tool does not link to it and marks its MOLST content unverified.
- The README documents the optional API keys (`PHN_CENSUS_API_KEY`, `PHN_NCBI_API_KEY`,
  `PHN_OPENFDA_API_KEY`, `PHN_AIRNOW_API_KEY`).

## [0.7.0] — 2026-11-08 (not tagged) — Phases 2 to 6: epidemiology engine, communicable disease, public data, immunizations, and immunization forecasting

### Added — Phase 2: epidemiology and biostatistics engine

- `age_adjust_rates` (EPI-02): direct age adjustment to the 2000 U.S. standard population using
  NCHS Master List weights, Fay–Feuer gamma intervals, and comparisons with a reference group.
- `analyze_two_by_two` (EPI-03): risk and odds ratios with Katz, Woolf, Fisher exact, and mid-p
  intervals; attributable fractions; chi-square and exact tests; Mantel–Haenszel and Breslow–Day.
- `calculate_sample_size` (EPI-04): surveys (finite population correction, design effect),
  two proportions (Fleiss with continuity correction), case-control, two means, power, clusters.
- `compute_standardized_ratio` (EPI-05): SIR/SMR with exact Poisson intervals and p-values.
- `analyze_trends` (EPI-09): APC, joinpoint-style segments and AAPC (BIC), interrupted time
  series, seasonality, Mann–Kendall, Durbin–Watson, and a trend chart.
- `run_statistical_test` (EPI-10): automatic test selection with assumption checks and
  sensitivity tests, on inline data or local files (aggregates only).
- `create_chart` (EPI-11): accessible SVG/PNG charts with a validated color-vision-safe palette,
  generated alt text, and a data table.
- `create_spc_chart` (EBP-06): run charts (Anhøj rules) and XmR, p, u, c control charts.
- `phn-epi` engine: GLMs by IRLS (Gaussian, Poisson, binomial, negative binomial), standardization,
  2×2 and stratified analysis, trend models, hypothesis tests (including Shapiro–Wilk), SPC, and
  sample size, with golden tests against R, SciPy, statsmodels, NCHS, and published tables. The
  R and Python scripts that produced each fixture are committed next to it.
- `phn-docs` chart renderer and accessible palette; `phn_data::socrata` client for data.cdc.gov.
### Added — Phase 3: communicable disease I

- `search_ma_disease_control_guide` (CD-02): offline tantivy search of a 1.1 MB plain-text corpus
  of 25 DPH guide chapters and 66 DPH fact sheet and guidance pages, built reproducibly by
  `cargo xtask build-guide-corpus` (URLs, file dates, and checksums in the snapshot manifest).
- `calculate_isolation_and_exclusion` (CD-03): a date-rule engine over a pending-review table of
  37 diseases (105 CMR 300.200, DPH guidance), with explicit day counting, settings, facts, and
  citations per rule.
- `analyze_outbreak_line_list` (CD-04): line-list column mapping, case definitions, suppressed
  person/place/time tables, exposure-specific attack rates, likely vehicles, exposure window.
- `generate_epi_curve` (CD-05): epidemic curves with incubation-based intervals, strata, exposure
  markers, and a described pattern; returned counts are suppressed.
- `plan_contact_tracing` (CD-06): infectious period, elicitation window, per-contact dates, and an
  XLSX contact-tracking template with date formulas.
- `assess_measles_exposure` (CD-07): evidence of immunity, PEP windows and IG doses, exclusion,
  priority groups.
- `detect_aberrations` (CD-10): EARS C1–C3, CUSUM, and Farrington-type (Noufaily) detection in
  `phn_epi::aberration`, golden-tested against R `surveillance`.
- `infection_control_for_congregate_settings` (CD-13): precautions, PPE, cohorting, and EPA
  disinfectant lists for congregate settings.
- The server's first MCP prompt, `investigate_outbreak`.
- Content (all pending review): `content/cd/disease_rules.yaml`, `measles_exposure.yaml`,
  `infection_control.yaml`; a fictional church-supper outbreak fixture for tests.
### Added — Phase 4: public data and evidence platform

- `build_community_health_profile` (EQ-01): ACS 5-year indicators (each cited by table ID, with
  margins of error and CV > 30% flagged), PLACES, and SVI for any Massachusetts town, county, or
  tract, compared with Massachusetts and the U.S.; works offline for all 351 towns.
- `get_social_vulnerability_index` (EQ-02): SVI 2022 tract ranks within Massachusetts or
  nationally, by theme.
- `language_access_profile` (EQ-03): LEP by language group (C16001, C16002, B16001) against the
  HHS safe harbor threshold, with margin-of-error-aware status. Content pending review; the
  safe harbor entry is low confidence.
- `measure_health_disparities` (EQ-04): rate ratios (exact conditional and log intervals), rate
  differences, index of disparity, between-group variance, excess events, and PAF; golden tests
  against R.
- `get_places_estimates` (CHR-01): CDC PLACES 2025 estimates with intervals and data years for
  counties, towns, places, tracts, and ZCTAs.
- `create_choropleth_map` (EPI-12): SVG/PNG choropleth maps with Fisher–Jenks or quantile
  classes, the accessible sequential palette, hatched suppressed areas, and alt text.
- `search_pubmed` (EBP-01) and `search_cdc_guidance` (EBP-02): NCBI E-utilities (no email,
  rate-limited, cached a day), the CDC content syndication API, and HAN notice pages.
- Data adapters (`phn_data::census`, `geography`, `boundaries`, `svi`, `places`, `eutils`,
  `cdc_guidance`, `apifetch`) with live → cache → snapshot fallback, and about 4 MB of new
  snapshots: ACS 2020–2024 indicators and detailed languages, SVI 2022, PLACES 2025, census
  geographies and crosswalks, and simplified town, tract, ZCTA, and school district boundaries,
  built reproducibly by `cargo xtask build-public-snapshots`.
### Added — Phase 5: immunizations I

- `get_immunization_schedule` (VAX-01) and `compare_vaccine_recommendations` (VAX-02): schedules
  and 17 recommendation topics with DPH, AAP, ACOG, AAFP, NEPHC, and CDC/ACIP positions, dates,
  and sources side by side; CDC minimum ages and intervals and contraindications.
- `check_ma_school_immunization_requirements` (VAX-04): a rule engine for the 2026–2027
  Massachusetts school requirements (child care/preschool, K–12, college) with dose validation,
  the 4-day grace period, live-vaccine spacing, proof of immunity, exemptions, earliest valid
  dates, and a roster mode that writes student results to `/data/output` and returns suppressed
  counts.
- `analyze_vaccination_coverage` (VAX-05): DPH school survey and CDC NIS/VaxView snapshots,
  herd-immunity thresholds with citations, school and town rankings, and local school counts.
- `vaccine_temperature_excursion_helper` (VAX-06), `lookup_vaccine_product` (VAX-07),
  `plan_vaccination_clinic` (VAX-08), and `vaccine_confidence_conversation_guide` (VAX-09).
- `lookup_medication_information` (CORE-07): RxNorm, openFDA (labels, recalls, shortages), and
  DailyMed; optional `PHN_OPENFDA_API_KEY`.
- New setting `vaccine_recommendation_basis` (in `configure_jurisdiction`), defaulting to DPH,
  then the professional societies, then CDC/ACIP.
- Immunization content under `content/vax/` (pending review), CDC CVX/MVX/trade name/NDC, DPH
  school survey, and NIS snapshots with their builder scripts in `xtask/scripts/`, and a
  fictional school roster in the synthetic corpus.
### Added — Phase 6: immunization forecasting and surveillance feeds

- `phn-cdsi` crate: CDC Clinical Decision Support for Immunization engine (Logic Specification
  v4.6) with the CDSi Supporting Data v4.65 embedded and checksummed; runs all 1,401 CDC test
  cases (all 1,064 healthy childhood and adult cases pass; 36 older underlying-condition cases
  are documented exceptions). Results: `docs/cdsi-test-results.md`.
- `forecast_immunizations_due` (VAX-03): dose evaluation, forecast dates, and a projected
  catch-up schedule for one person, or suppressed counts with per-person files written to
  `/data/output` for a file of many people.
- `get_notifiable_disease_trends` (CD-08): NNDSS weekly counts (data.cdc.gov `x9gk-5huc`)
  for Massachusetts, New England, and the U.S. compared with the five previous MMWR years (year
  to date and last 4 weeks), provisional year-end totals, and an all-conditions overview.
- `get_respiratory_and_wastewater_surveillance` (CD-09): weekly COVID-19, influenza, and RSV
  briefing from NHSN, NSSP, NREVSS, the ARI activity level, and NWSS wastewater levels by
  Massachusetts site, with data dates and lag, MWRA's final Deer Island values, and DPH's
  dashboard link.
- Bundled snapshots for both (about 260 KB), built by the new
  `cargo xtask build-surveillance-snapshots`; `cargo xtask build-snapshots` now keeps manifest
  entries it does not build.
### Changed — reference content re-checked against live sources

- Reportable conditions (`content/cd/reportable_conditions.yaml` 0.2.0) re-checked against the
  text of 105 CMR 300.000 (Mass. Register #1470, 5/27/22) and DPH's current provider and
  laboratory reporting lists: corrected timeframes, reporters, and recipients (for example,
  pertussis, mumps, brucellosis, glanders, melioidosis, and Q fever are not on DPH's
  immediate-call list; invasive group A strep, tetanus, HUS, and active TB are; TB, HIV, and
  STIs go directly to DPH); added 35 conditions, including alpha-gal syndrome (temporary
  declaration from April 1, 2026); removed leptospirosis and Candida auris, which the regulation
  does not name. Every entry now has a `citation`, shown in `lookup_reportable_condition`
  answers. Still pending subject-matter review.
- `lookup_reportable_condition` now tells providers to call the DPH Epidemiology Line for
  immediately reportable conditions and names the board of health where the case is diagnosed
  (not the town of residence), as 105 CMR 300.100 says.
- HMCC regions: Wilmington moved from 4AB to Region 3 to match DPH's HMCC Regional Boundaries
  map; the snapshot builder applies an override table.
- Data source registry: fixed the 105 CMR 300.000 address (the old mass.gov URL returns 404).

## [0.2.0] — 2026-11-07 (not tagged) — Phase 1: local data, privacy guardrails, and documents

### Added
- `read_local_file` (CORE-04): lists `/data` and extracts text and tables from PDF, DOCX, XLSX,
  XLS, ODS, CSV, HTML, Markdown, and text, with identifier warnings and redaction.
- `search_agency_policies` (CORE-05): full-text search of the agency's policies with file and
  section or page citations; index rebuilt automatically when files change.
- `export_document` (CORE-06): Markdown to accessible DOCX and tagged PDF with letterhead, XLSX,
  CSV, SVG, and PNG; local mail-merge.
- `apply_small_cell_suppression` (EPI-06): primary and complementary suppression.
- `profile_local_dataset` (EPI-07): structure, types, missingness, and identifier warnings;
  optional suppressed value summaries.
- `deidentify_dataset` (EPI-08): HIPAA Safe Harbor de-identification or limited data set, with a
  rare-combination check and a written report.
- `check_document_accessibility` (COMM-05): Section 508 / WCAG 2.1 AA checks for DOCX, HTML, PDF.
- PHI detector using column names, value patterns, and U.S. Census surname and SSA first-name
  lists; automatic strict redaction of documents that contain client identifiers.
- Synthetic test corpus with 266 seeded identifiers and MCP integration tests proving that no
  row-level data is returned by default and that exported documents pass the server's own
  accessibility checks.

## [0.1.0] — 2026-11-07 (not tagged) — Phase 0: foundation (internal)

### Added
- Rust workspace (`phn-core`, `phn-content`, `phn-data`, `phn-epi`, `phn-local`, `phn-docs`,
  `phn-tools`, `phn-server`, `xtask`) and an `rmcp` stdio server.
- Response envelope with markdown summary, structured data, sources, caveats, and privacy block;
  input and output JSON Schemas and annotations on every tool.
- Toolsets and role profiles, switchable at runtime with a `tools/list_changed` notification.
- Configuration through environment variables and `/cache/phn-settings.toml`.
- HTTP client with proxy and custom CA support, SQLite cache, live → cache → snapshot fallback,
  and a checksummed snapshot store.
- Massachusetts reference snapshots built from MassGIS: 351 municipalities with county, DPH
  preparedness region, and HMCC region; 402 school districts.
- Reference content format (YAML + JSON Schema) with review metadata; 105 CMR 300.000 reportable
  conditions (draft, pending review) and the data source registry.
- Tools: `get_data_source_status` (CORE-08), `configure_jurisdiction` (CORE-09),
  `calculate_rates` (EPI-01), `lookup_reportable_condition` (CD-01).
- Dockerfile (distroless, non-root, read-only root filesystem), GitHub Actions for CI, releases
  (multi-architecture GHCR image, SBOM, vulnerability scan, cosign signature, offline tarballs),
  and weekly snapshot freshness checks.
