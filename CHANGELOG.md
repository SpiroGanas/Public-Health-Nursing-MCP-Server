# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/). Version numbers track the phases in
[`project_plan.md`](project_plan.md).

## [Unreleased]

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
<!-- Phase 2 entries go above this line. -->

### Added — Phase 3: communicable disease I

<!-- Phase 3 entries go above this line. -->

### Added — Phase 4: public data and evidence platform

<!-- Phase 4 entries go above this line. -->

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
<!-- Phase 5 entries go above this line. -->

### Added — Phase 6: immunization forecasting and surveillance feeds

<!-- Phase 6 entries go above this line. -->

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
