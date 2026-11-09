# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/). Version numbers track the phases in
[`project_plan.md`](project_plan.md).

## [Unreleased]

### Added

- User guide for public health nurses (`docs/user-guide/PHN-MCP-Server-User-Guide.docx`): an
  executive summary of the server's benefits, how it works and protects privacy, getting
  started, and for each of the 47 implemented tools what it does, when to use it, how to ask for
  it, what it returns, and what to keep in mind; plus a tool index, glossary, and
  troubleshooting. Built from `docs/user-guide/guide-content.js` by `build-user-guide.js`; a
  registry test keeps the guide's tool list in step with the implemented tools.

### Added — Phase 7: school health

<!-- Phase 7 entries go below this line. -->

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

<!-- End of Phase 7 entries. -->

### Added — Phase 8: maternal, child, and family health

<!-- Phase 8 entries go below this line. -->
<!-- End of Phase 8 entries. -->

### Added — Phase 9: PHN practice toolkit and case management

<!-- Phase 9 entries go below this line. -->
<!-- End of Phase 9 entries. -->

### Added — Phase 10: healthy aging and mental health screening

<!-- Phase 10 entries go below this line. -->

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
<!-- End of Phase 10 entries. -->

### Added — Phase 11: substance use, crisis resources, and health communication

<!-- Phase 11 entries go below this line. -->
<!-- End of Phase 11 entries. -->

### Added — Phase 12: emergency preparedness I: situational awareness and planning

<!-- Phase 12 entries go below this line. -->
<!-- End of Phase 12 entries. -->

## [0.7.0] — Phases 2 to 6: epidemiology engine, communicable disease, public data, immunizations, and immunization forecasting

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

## [0.2.0] — Phase 1: local data, privacy guardrails, and documents

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

## [0.1.0] — Phase 0: foundation (internal)

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
