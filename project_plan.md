# Public Health Nursing MCP Server: Project Plan

This plan recommends an order for building the tools brainstormed in [`brainstorming.md`](brainstorming.md). It assumes the server is written in **Rust** and distributed to end users as a **Docker container**.

## 1. Summary

- **Scope.** The plan builds all **148 in-scope tools** from `brainstorming.md`. The **24 tools that need outside resources** are listed in [Appendix A](#appendix-a-tools-that-require-outside-resources) and are not scheduled.
- **Schedule.** There are **20 phases (Phase 0–19)**. Each is about one month of work for one developer, so v1.0 arrives after about 20 months.
- **Order.** The foundation and privacy guardrails come first, then the shared calculation and data layers. Themed tool groups follow, ordered by how often a Massachusetts public health nurse needs them. Leadership and policy tools come last, because they mostly assemble the outputs of earlier tools.
- **Pilots.** Real nurses use the server in three pilots: after Phase 3 (month 4, communicable disease), after Phase 7 (month 8, immunization and school health), and after Phase 13 (month 14, emergency preparedness).

## 2. Assumptions

| Topic | Assumption |
|---|---|
| Team | One full-time Rust developer. A phase is about 4 weeks (about 20 working days), including tests, documentation, and a release. |
| Subject matter expert | The DNP-prepared public health nurse is the product owner and subject matter expert. They review reference content and accept each phase, at roughly 2–4 hours per week. This is not counted as an outside resource. |
| Jurisdiction | Massachusetts-specific content (DPH, 105 CMR, 244 CMR, M.G.L., 351 cities and towns), with federal data for comparison. |
| Data | Public and aggregate data, plus files the nurse mounts into the container. No connections to systems that hold PHI. |
| Deployment | Each nurse runs the container locally. An MCP client such as Claude Desktop launches it over stdio. |
| Network | Live public APIs, with bundled snapshots as an offline fallback. |
| API keys | Free keys the nurse registers for themselves (Census, AirNow, USPSTF, Regulations.gov, NCBI) are configuration, not outside resources. |

## 3. What counts as an "outside resource"

A tool is **in scope** if one developer, working with the nurse, can build it from:
- public APIs and datasets;
- published federal and Massachusetts guidance, regulations, and forms;
- free-to-use instruments and frameworks;
- open-source libraries;
- the nurse's own files.

A tool goes to **Appendix A** if it cannot be built without one of the following:
1. **Purchased or licensed content or software**, such as CPT codes, ASQ-3, AGS Beers Criteria, UpToDate, CINAHL, or ArcGIS.
2. **Access agreements with state or agency systems**, such as MIIS, MAVEN, NSSP ESSENCE, the Public Health Data Warehouse, or a district student information system.
3. **Professional sign-off needed to build it responsibly**: legal counsel, a TB physician or pharmacist, or a qualified statistician.
4. **Privacy approval.** Some tools must put identifiable client information into the AI model's context, such as names and addresses for route planning or caseload management. These need agency privacy approval and a business associate agreement (BAA) with the AI vendor.

## 4. Ordering principles

1. **Foundation before features.** The MCP server, Docker image, configuration, caching, and citation format must exist before any tool.
2. **Privacy guardrails before local data.** File sandboxing, PHI detection, small-cell suppression, and the "local mail-merge" pattern ship before any tool that reads the nurse's files.
3. **Shared engines before the tools that use them.** The epidemiology engine comes before outbreak tools, data adapters before community profiles, and the document generator before plan builders.
4. **Most frequent work first.** Massachusetts boards of health must investigate reportable diseases, so communicable disease comes first. Immunization and school health follow.
5. **Lower-risk before higher-risk.** Reference lookups ship before clinical calculators. The immunization forecaster (CDSi) gets its own phase so it can be validated against CDC's test cases.
6. **Group by shared adapters and content.** Tools that use the same data source or regulation set are built together to avoid rework.
7. **Composition last.** Community health assessments, grant narratives, strategic plans, and policy briefs mostly assemble outputs from earlier tools, so they come late.

## 5. Architecture

This section describes the architecture as built through Phase 14 (version 0.15.0). Where the build departed from the original plan, the architecture decision record (ADR) in [`docs/adr/`](docs/adr/) that made the change is named. [`STATUS.md`](STATUS.md) has the current state of the work.

### 5.1 Cargo workspace

```
crates/
  phn-server     # binary: rmcp server, stdio transport, toolset filtering, MCP prompts and resources
  phn-core       # configuration, errors, response envelope, toolsets and profiles
  phn-data       # HTTP client (proxy + custom CA), SQLite cache, snapshot store, API adapters
  phn-content    # reference content (YAML + JSON Schema) with review metadata, embedded in the binary
  phn-epi        # rates, CIs, standardization, 2x2, regression, trends, tests, aberration detection,
                 # SPC, queueing, survey estimation
  phn-cdsi       # CDC CDSi supporting data and the immunization evaluation and forecasting engine
                 # (added in Phase 6; ADR 0010)
  phn-local      # sandboxed /data access, CSV/XLSX/PDF/DOCX parsing, PHI detection, de-identification,
                 # full-text index of agency policies
  phn-docs       # DOCX/XLSX/PDF/CSV/SVG/PNG output, letterhead, mail-merge, accessibility checks,
                 # chart and map rendering
  phn-tools      # tool implementations, one module per theme (vax, cd, school, mch, ...)
content/         # reference content with citations and review metadata
snapshots/       # dated data snapshots + manifest
xtask/           # snapshot builders (Rust, and Python in scripts/), content validator
```

### 5.2 Main Rust dependencies

| Need | Crate(s) as built |
|---|---|
| MCP protocol | `rmcp` (official Rust SDK): tools with JSON Schema via `schemars`, structured output, tool annotations such as `readOnlyHint` (ADR 0001) |
| Async, HTTP, serialization | `tokio`, `reqwest` (rustls; honors `HTTPS_PROXY`; extra CA bundle for agencies that inspect TLS traffic), `serde`, `serde_json`, `serde-saphyr` (YAML content), `toml` (saved settings) |
| Cache and search | `rusqlite` (bundled SQLite), `tantivy` (offline full-text search) |
| Tabular data | `csv`, `calamine` (XLSX read), `rust_xlsxwriter` (XLSX write). `polars` was not needed. |
| Documents | The server's own DOCX writer and tagged-PDF writer on `lopdf`, with `subsetter` and `skrifa` for the bundled fonts; `lopdf` also reads PDF text. ADR 0003 chose this over `docx-rs`, `printpdf`, and `typst` so that every accessibility property is controlled. |
| Statistics | `statrs` for distributions; the methods themselves are in `phn-epi` |
| Charts and maps | Drawn by `phn-docs` as accessible SVG and turned into PNG with `resvg`. ADR 0006 chose this over `plotters`. |
| Geospatial | Simplified boundaries bundled as snapshots and read by the server's own code. `geo` and `shapefile` are used only by `xtask` to build the snapshots; `geojson` and `rstar` were not needed. |
| CDSi supporting data | `quick-xml` |
| Content validation | `jsonschema` |
| Logging | `tracing`, writing **to stderr only** (stdout carries the MCP protocol) |

### 5.3 Response contract

Every tool returns a markdown `summary` for the model and a structured `data` object (MCP structured content), plus (ADR 0002):
- `sources[]`: title, publisher, URL, effective date, retrieval date, and mode (`live`, `cache`, `snapshot`, `local`, or `computed`);
- `caveats[]`: data limits, small numbers, and recommendation divergence;
- `privacy`: whether any row-level data is included, whether suppression was applied and at what threshold, which kinds of identifiers were detected, and which files were written;
- a decision-support `notice` on clinical and legal answers.

Reference content and snapshots are also exposed as MCP **resources**. Common multi-tool workflows are exposed as MCP **prompts**. Two exist so far: "investigate an outbreak" (Phase 3) and "plan a tabletop exercise" (Phase 13). Others, such as "build a community health assessment" and "prepare a grant application", come with their phases.

### 5.4 Toolsets and role profiles

Exposing 148 tools at once would crowd the model's context and hurt tool selection. Tools are therefore grouped into toolsets: `core`, `cd`, `vax`, `school`, `mch`, `aging`, `bh`, `epr`, `env`, `chronic`, `equity`, `epi`, `planning`, `leadership`, `policy`, `comms`, and `ebp`. They are enabled through `PHN_TOOLSETS` or role profiles set with `PHN_PROFILE`: `school_nurse`, `cd_nurse`, `phn_leader`, and `preparedness`. The `mch` and `aging` toolsets are not in any profile yet; they are on when no profile is set or when they are named in `PHN_TOOLSETS`.

**Planned, not yet built:** a set of realistic nurse prompts that is re-run at every release to check that the model picks the right tools. Today the per-phase acceptance scripts in `docs/acceptance/` are run by hand.

### 5.5 Privacy and safety guardrails

- **Sandbox.** The container reads and writes only `/data` (mounted by the nurse) and `/cache`. Paths are canonicalized to block traversal (ADR 0004).
- **Aggregate first.** Tools that read local data return aggregates by default and apply small-cell suppression automatically. Returning row-level data requires an explicit argument, and the rows are always masked; there is no option to turn masking off (ADR 0004).
- **Local mail-merge and local fill.** Anything identifiable, such as referral letters, contact sheets, or rosters, is written to `/data/output` and never returned to the model. Documents about one person are built from a template that the model sees and a local file that it does not (ADR 0008). Staff phone numbers and emails come only from a roster file (ADR 0009).
- **No telemetry.** The server sends no usage data anywhere. It calls only the public data sources listed in `content/data_sources.yaml` and reported by `get_data_source_status`, and it runs fully offline with `PHN_OFFLINE=1` or `docker run --network none`. The HTTP client does not itself enforce a list of allowed hosts; whether to add that check in code is an open decision.
- **Decision support only.** Clinical and legal tools always cite sources and include a decision-support notice. No tool submits anything to an external system.

### 5.6 Offline fallback and data freshness

- Each data adapter tries the live API first, then the cache (with a time-to-live), then the bundled snapshot. Every response states which mode was used and the data's date.
- Snapshots are built by `cargo xtask` subcommands and the Python scripts in `xtask/scripts/`, which record each source, its vintage, and SHA-256 checksums in `snapshots/manifest.json`. A weekly CI job runs `cargo xtask build-snapshots --check` (the MassGIS reference lists) and `cargo xtask validate-content`. The other builders are run by hand when a source publishes new data.
- This matters because agency firewalls can block public APIs, and some federal datasets were removed or changed in 2025.

### 5.7 Docker image and distribution

- **Build.** A multi-stage build (using `cargo-chef` for dependency caching) produces a `gcr.io/distroless/cc-debian12` runtime image that runs as a non-root user with a read-only root filesystem (ADR 0001).
- **Publishing.** The release workflow builds multi-architecture images (amd64 and arm64, with buildx), pushes them to GHCR with semantic version tags, and adds an SBOM, a vulnerability scan (`trivy`), and a `cosign` signature. It runs when a version is tagged. No version has been tagged yet, so the workflow has not run and no image has been published.
- **Data.** Reference content and snapshots are embedded in the server binary at build time, so the image works offline and ships exactly what was tested (ADR 0001). Newer data fetched from live sources is kept in the `/cache` volume.
- **Time zone.** The image sets `TZ=America/New_York`, so "today" and the dates and times on documents are Eastern time; `-e TZ=...` changes it (ADR 0010).
- **Restricted networks.** For networks that block GHCR, the release workflow attaches a `docker save` tarball for each architecture.

Example client configuration (Claude Desktop, macOS). The README has the current version of this, with the Windows form:

```json
{
  "mcpServers": {
    "public-health-nursing": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm", "--read-only",
        "-v", "/Users/nurse/PHN-Data:/data",
        "-v", "phn-cache:/cache",
        "-e", "PHN_PROFILE=cd_nurse",
        "ghcr.io/spiroganas/public-health-nursing-mcp-server:0.14.0"
      ]
    }
  }
}
```

Docker Desktop requires a paid subscription for larger organizations, which may include state and municipal agencies. Podman Desktop and Rancher Desktop are free alternatives. This should be raised with agency IT in Phase 0.

### 5.8 Quality

- **Golden tests** compare every calculator with published worked examples or reference outputs: NCHS age adjustment, exact Poisson CI tables, the CDC CDSi test cases, and reference series for aberration detection. Reference outputs from R, SciPy, and statsmodels are generated during development and committed as fixtures with the scripts that made them.
- **MCP integration tests** use an `rmcp` client to check schema validity, errors, and annotations for every tool, and a leak test checks that no seeded identifier from the synthetic corpus is returned. Manual testing with Claude Desktop on Windows and macOS is part of each phase's acceptance script and has not been done yet.
- **Content metadata.** Every content file records its sources and a `meta.review` block (`status`, `reviewed_by`, `reviewed_on`, `review_due`). CI fails when a review is overdue.
- **Code checks.** CI runs `cargo fmt`, `cargo clippy -D warnings`, `cargo deny`, and `cargo audit`.

### 5.9 Definition of done (every phase)

1. Each tool has an input/output schema, annotations, and a description written for the model.
2. Unit, golden, and MCP integration tests pass in CI.
3. Reference content has citations and has been reviewed by the nurse.
4. The tool reference documentation and changelog are updated.
5. A versioned, signed multi-architecture image is published to GHCR.
6. The nurse has accepted the phase using a short script of realistic prompts.

Phases 0–13 are built. Items 3, 5, and 6 are still open for every one of them: the reference content is drafted with citations but has not yet been reviewed by the nurse, no signed image has been published, and no acceptance script has been run. `STATUS.md` tracks these open items.

## 6. Roadmap at a glance

| Phase | Month | Theme | Tools | Cumulative | Release |
|---|---|---|---|---|---|
| 0 | 1 | Foundation | 4 | 4 | v0.1 (internal) |
| 1 | 2 | Local data, privacy guardrails, and documents | 7 | 11 | v0.2 |
| 2 | 3 | Epidemiology and biostatistics engine | 8 | 19 | v0.3 |
| 3 | 4 | Communicable disease I | 8 | 27 | **v0.4: Pilot 1** |
| 4 | 5 | Public data and evidence platform | 8 | 35 | v0.5 |
| 5 | 6 | Immunizations I | 9 | 44 | v0.6 |
| 6 | 7 | Immunization forecasting and surveillance feeds | 3 | 47 | v0.7 |
| 7 | 8 | School health | 8 | 55 | **v0.8: Pilot 2** |
| 8 | 9 | Maternal, child, and family health | 8 | 63 | v0.9 |
| 9 | 10 | PHN practice toolkit and case management | 8 | 71 | v0.10 |
| 10 | 11 | Healthy aging and mental health screening | 9 | 80 | v0.11 |
| 11 | 12 | Substance use, crisis resources, and health communication | 9 | 89 | v0.12 |
| 12 | 13 | Emergency preparedness I: situational awareness and planning | 8 | 97 | v0.13 |
| 13 | 14 | Emergency preparedness II: ICS, plans, and exercises | 5 | 102 | **v0.14: Pilot 3** |
| 14 | 15 | Environmental health and climate | 8 | 110 | v0.15 |
| 15 | 16 | Chronic disease prevention and community assessment | 8 | 118 | v0.16 |
| 16 | 17 | Program planning, evaluation, and evidence | 9 | 127 | v0.17 |
| 17 | 18 | Grants and finance | 7 | 134 | v0.18 |
| 18 | 19 | Leadership, accreditation, and professional practice | 9 | 143 | v0.19 |
| 19 | 20 | Policy and law; v1.0 hardening | 5 | 148 | **v1.0** |

Phases differ in tool count because the tools differ in size. A phase with few tools contains one large item, such as the CDSi forecaster, the ICS form set, or the offline geocoder.

## 7. Phase details

### Phase 0: Foundation (Month 1)

**Goal:** An installable MCP server that proves the architecture end to end.

**Tools:** CORE-08 `get_data_source_status`, CORE-09 `configure_jurisdiction`, EPI-01 `calculate_rates`, CD-01 `lookup_reportable_condition`.

**Engineering:**
- Cargo workspace, `rmcp` stdio server, toolset/profile registry, and response envelope with citations.
- Configuration through environment variables and a config file in `/cache`. Massachusetts reference lists: the 351 towns, counties, HMCC regions, and school districts.
- HTTP client with proxy and custom CA support, SQLite cache, and the snapshot store and manifest format.
- Content format (YAML + JSON Schema) with review metadata. The first content item is the 105 CMR 300.000 reportable disease list.
- Dockerfile, GitHub Actions CI (lint, test, deny/audit, multi-arch build, GHCR push), and a README with client setup for Windows and macOS.

**Exit:** The nurse pulls the image and runs all four tools from Claude Desktop on a Windows machine and a Mac. Agency IT has been asked about container runtime approval.

### Phase 1: Local data, privacy guardrails, and documents (Month 2)

**Goal:** Read the nurse's files safely and produce professional documents.

**Tools:** CORE-04 `read_local_file`, CORE-05 `search_agency_policies`, CORE-06 `export_document`, EPI-06 `apply_small_cell_suppression`, EPI-07 `profile_local_dataset`, EPI-08 `deidentify_dataset`, COMM-05 `check_document_accessibility`.

**Engineering:**
- Sandboxed file access and parsers for CSV, XLSX, DOCX, and PDF text.
- A PHI detector combining column-name heuristics, patterns, and public name lists.
- A Safe Harbor de-identifier and the suppression engine.
- A `tantivy` index for agency policies.
- DOCX, XLSX, and PDF writers with letterhead templates, and the local mail-merge pattern.

**Exit:**
- On a synthetic test corpus, the PHI detector flags the seeded identifiers.
- Integration tests prove that no row-level data is returned by default.
- Exported DOCX files pass the server's own accessibility checks.

### Phase 2: Epidemiology and biostatistics engine (Month 3)

**Goal:** The shared, tested calculation layer that later phases depend on.

**Tools:** EPI-02 `age_adjust_rates`, EPI-03 `analyze_two_by_two`, EPI-04 `calculate_sample_size`, EPI-05 `compute_standardized_ratio`, EPI-09 `analyze_trends`, EPI-10 `run_statistical_test`, EPI-11 `create_chart`, EBP-06 `create_spc_chart`.

**Engineering:**
- The `phn-epi` crate: exact Poisson and binomial CIs, gamma-method CIs, and the 2000 US standard population.
- GLM fitting (IRLS) for Poisson, negative binomial, and logistic regression; segmented regression; SPC rules.
- Charts with an accessible palette and generated alt text. The plan named `plotters`; the charts were built as the server's own SVG renderer instead (ADR 0006).

**Exit:** All golden tests match their published or reference values within tolerance.

### Phase 3: Communicable disease I (Month 4) → Pilot 1

**Goal:** Support the investigation and control work that Massachusetts boards of health do every day.

**Tools:** CD-02 `search_ma_disease_control_guide`, CD-03 `calculate_isolation_and_exclusion`, CD-04 `analyze_outbreak_line_list`, CD-05 `generate_epi_curve`, CD-06 `plan_contact_tracing`, CD-07 `assess_measles_exposure`, CD-10 `detect_aberrations`, CD-13 `infection_control_for_congregate_settings`.

**Engineering:**
- Ingest DPH's *Guide to Surveillance, Reporting and Control* and its fact sheets into the content index.
- Build a per-disease table of incubation and infectious periods, and a date-rule engine for isolation and exclusion.
- Implement EARS C1–C3, CUSUM, and Farrington-type algorithms, plus line-list column mapping.
- Add an "investigate an outbreak" MCP prompt.

**Content review:** The nurse verifies the isolation and exclusion rules for the 30 most commonly investigated diseases.

**Release:** Pilot 1 with a small group of nurses on real, de-identified work.

### Phase 4: Public data and evidence platform (Month 5)

**Goal:** Build the data adapters behind community profiles, maps, and evidence searches.

**Tools:** EQ-01 `build_community_health_profile`, EQ-02 `get_social_vulnerability_index`, EQ-03 `language_access_profile`, EQ-04 `measure_health_disparities`, CHR-01 `get_places_estimates`, EPI-12 `create_choropleth_map`, EBP-01 `search_pubmed`, EBP-02 `search_cdc_guidance`.

**Engineering:**
- Adapters for the Census ACS API, the Socrata client for data.cdc.gov, the SVI snapshot, NCBI E-utilities, and MMWR/HAN feeds.
- MassGIS boundaries (towns, tracts, ZCTAs, school districts) with crosswalks, and a choropleth renderer.

**Exit:** A community profile works for any of the 351 towns both live and offline, and every figure is cited.

### Phase 5: Immunizations I (Month 6)

**Goal:** Immunization reference, school compliance, and clinic tools.

**Tools:** VAX-01 `get_immunization_schedule`, VAX-02 `compare_vaccine_recommendations`, VAX-04 `check_ma_school_immunization_requirements`, VAX-05 `analyze_vaccination_coverage`, VAX-06 `vaccine_temperature_excursion_helper`, VAX-07 `lookup_vaccine_product`, VAX-08 `plan_vaccination_clinic`, VAX-09 `vaccine_confidence_conversation_guide`, CORE-07 `lookup_medication_information`.

**Engineering:**
- A schedule content model, and recommendation content from each issuing body with effective dates.
- A rules engine for Massachusetts school requirements that reuses the Phase 3 date logic.
- A DailyMed/openFDA/RxNorm adapter, a loader for CDC CVX/MVX code sets, and NIS and DPH school survey snapshots.

**Content review:** The nurse checks the school requirements against DPH's current document and sets the annual refresh date.

### Phase 6: Immunization forecasting and surveillance feeds (Month 7)

**Goal:** A validated catch-up and forecasting engine. This is the plan's largest single clinical item.

**Tools:** VAX-03 `forecast_immunizations_due`, CD-08 `get_notifiable_disease_trends`, CD-09 `get_respiratory_and_wastewater_surveillance`.

**Engineering:**
- Parse the CDSi supporting data (antigen series and schedules) and implement the evaluation and forecasting logic.
- Run the full CDC CDSi test-case suite in CI and publish the pass rate.
- The NNDSS, respiratory, and wastewater adapters reuse the Phase 4 Socrata client.

**Exit:** All applicable CDSi test cases pass, or each exception is documented. Output tells the nurse to verify against MIIS before acting.

**Risk:** If CDSi overruns, CD-08 and CD-09 move to Phase 7.

### Phase 7: School health (Month 8) → Pilot 2

**Goal:** Full school nurse support, building on the immunization tools.

**Tools:** SCH-01 `get_ma_school_screening_requirements`, SCH-02 `track_school_screenings`, SCH-03 `school_medication_administration_guide`, SCH-04 `build_individualized_health_plan`, SCH-05 `concussion_return_to_learn_tracker`, SCH-06 `monitor_school_absenteeism`, SCH-07 `build_school_health_annual_report`, SCH-08 `child_care_health_consultation_checklist`.

**Engineering:**
- 105 CMR 200/201/210 and 606 CMR 7.00 content, and school-year calendar logic.
- Roster and screening file templates.
- Multilingual referral letters produced through local mail-merge.
- Absenteeism spike detection that reuses Phase 3 aberration detection.
- An annual report template that matches DPH's format.

**Release:** Pilot 2 with school nurses, ideally timed before a school year starts.

### Phase 8: Maternal, child, and family health (Month 9)

**Goal:** Home visiting, early childhood, and lead prevention support.

**Tools:** MCH-01 `plan_newborn_postpartum_visit`, MCH-02 `check_developmental_milestones`, MCH-03 `calculate_growth_percentiles`, MCH-04 `childhood_lead_screening_guide`, MCH-05 `map_lead_risk`, MCH-06 `maternal_infant_health_profile`, MCH-07 `mandated_reporter_51a_guide`, EQ-06 `estimate_benefit_eligibility`.

**Engineering:**
- WHO and CDC LMS tables and corrected-age math.
- 105 CMR 460.000 content and the high-risk community list.
- Vital statistics and PRAMS snapshots, and federal poverty guideline tables with an annual refresh.

**Content review:** The nurse reviews the milestone, lead, and mandated reporting content.

### Phase 9: PHN practice toolkit and case management (Month 10)

**Goal:** Everyday public health nursing documentation and case-management support.

**Tools:** CORE-01 `omaha_system_coder`, CORE-02 `classify_intervention_wheel`, CORE-03 `nursing_documentation_template`, CD-11 `plan_tb_contact_investigation`, CD-12 `ltbi_treatment_calendar`, EQ-05 `screen_health_related_social_needs`, EQ-07 `find_community_resources`, EQ-11 `newcomer_health_navigator`.

**Engineering:**
- Omaha System and Intervention Wheel taxonomies.
- TB contact investigation rules and LTBI regimen calendars.
- AHC HRSN scoring, a local resource directory schema, the HRSA adapter, and refugee and immigrant health content.

### Phase 10: Healthy aging and mental health screening (Month 11)

**Goal:** Older adult services and validated screening support.

**Tools:** AGE-01 `steadi_fall_risk_assessment`, AGE-02 `find_ma_aging_services`, AGE-03 `elder_protective_services_guide`, AGE-04 `explain_older_adult_coverage_options`, AGE-05 `map_older_adult_vulnerability`, AGE-06 `nursing_home_quality_lookup`, AGE-07 `advance_care_planning_toolkit`, MH-01 `find_crisis_resources`, MH-02 `score_mental_health_screen`.

**Engineering:**
- A town-to-service directory framework with snapshot builder, covering ASAPs, Councils on Aging, SHINE, and CBHCs.
- The CMS provider data adapter and STEADI norms.
- A screening-instrument scoring engine with safety-alert rules.
- AGE-05 reuses the Phase 4 ACS adapter.

**Content review:** The nurse validates screening cutoffs, safety messages, and the license terms of each instrument, and spot-checks the directories.

### Phase 11: Substance use, crisis resources, and health communication (Month 12)

**Goal:** Overdose prevention resources and the communication tools other phases will use.

**Tools:** MH-03 `get_overdose_data`, MH-04 `find_naloxone_and_harm_reduction`, MH-05 `find_substance_use_treatment`, MH-06 `plan_school_postvention`, MH-07 `get_youth_risk_behavior_data`, COMM-01 `check_readability`, COMM-02 `translate_health_material`, COMM-03 `create_social_media_campaign`, COMM-04 `draft_press_materials`.

**Engineering:**
- DPH town-level overdose snapshots, CDC provisional counts, the FindTreatment.gov adapter, and YRBS/YHS snapshots.
- Readability metrics and a CDC Clear Communication Index rubric.
- A glossary-based translation draft workflow with a back-translation check.
- Safe-messaging rules, shared with MH-06.

### Phase 12: Emergency preparedness I: situational awareness and planning (Month 13)

**Goal:** Hazard awareness, vulnerable-population data, and response modeling.

**Tools:** EPR-02 `model_dispensing_site_throughput`, EPR-03 `functional_needs_shelter_intake`, EPR-04 `get_electricity_dependent_population`, EPR-05 `get_active_hazard_alerts`, EPR-06 `hazard_vulnerability_assessment`, EPR-11 `plan_casper_assessment`, ENV-01 `heat_health_action_planner`, ENV-02 `air_quality_protective_guidance`.

**Engineering:**
- Adapters for the NWS API, OpenFEMA, the National Risk Index, emPOWER, and AirNow.
- A multi-server queueing model for dispensing sites.
- CASPER two-stage cluster sampling with census block data and weighting.

### Phase 13: Emergency preparedness II: ICS, plans, and exercises (Month 14) → Pilot 3

**Goal:** The documents a health department produces before, during, and after an incident.

**Tools:** EPR-01 `generate_ics_forms`, EPR-07 `continuity_of_operations_planner`, EPR-08 `design_hseep_exercise`, EPR-09 `build_after_action_report`, EPR-10 `crisis_risk_communication_builder`.

**Engineering:**
- ICS form templates with DOCX and PDF filling, and Incident Action Plan assembly. The form set is large, so this phase has fewer tools.
- COOP, HSEEP, and after-action report templates, PHEP capability mapping, and CERC content.

**Release:** Pilot 3, tested during a tabletop exercise designed with EPR-08.

### Phase 14: Environmental health and climate (Month 15)

**Goal:** EHS565-aligned environmental and climate-health tools.

**Tools:** ENV-03 `drinking_water_quality_lookup`, ENV-04 `environmental_justice_screen`, ENV-05 `climate_health_vulnerability_profile`, ENV-06 `vector_borne_disease_risk`, ENV-07 `housing_conditions_health_guide`, ENV-08 `chemical_exposure_reference`, CHR-06 `food_access_mapper`, CHR-07 `community_water_fluoridation_status`.

**Engineering:**
- The EPA Envirofacts (SDWIS) adapter, MassDEP PFAS snapshot, Massachusetts EJ population layer, and archived EJScreen snapshot.
- FEMA flood zone summaries and seasonal mosquito- and tick-borne disease snapshots.
- 105 CMR 410.000 content, ATSDR/NIOSH content, the USDA Food Access Atlas, and the fluoridation list.

### Phase 15: Chronic disease prevention and community assessment (Month 16)

**Goal:** Prevention programs and the full community health assessment workflow.

**Tools:** CHR-02 `community_bp_screening_toolkit`, CHR-03 `diabetes_prevention_finder`, CHR-04 `preventive_services_lookup`, CHR-05 `asthma_home_assessment`, EQ-08 `community_health_assessment_builder`, EQ-09 `prioritize_health_issues`, EQ-10 `analyze_qualitative_data`, EPI-13 `geocode_and_aggregate_addresses`.

**Engineering:**
- The USPSTF API adapter and the National DPP registry snapshot.
- A MAPP 2.0 "community health assessment" MCP prompt that chains EQ-01 through EQ-10, plus the Hanlon calculator.
- A qualitative pipeline with name scrubbing.
- An offline geocoder built on MassGIS address points and TIGER data. This is the largest item in the phase. If it makes the image too large, it may ship as an optional image variant.

### Phase 16: Program planning, evaluation, and evidence (Month 17)

**Goal:** The N750 planning and evaluation workflow and the evidence-appraisal chain.

**Tools:** PLAN-01 `build_logic_model`, PLAN-02 `write_smartie_objectives`, PLAN-03 `find_evidence_based_interventions`, PLAN-04 `apply_behavior_change_theory`, PLAN-05 `build_evaluation_plan`, PLAN-11 `track_program_performance`, EBP-03 `appraise_evidence`, EBP-04 `build_evidence_synthesis_table`, EBP-05 `plan_quality_improvement`.

**Engineering:**
- The Healthy People 2030 API adapter and Community Guide / *What Works for Health* snapshots.
- A library of theory constructs, and logic model diagrams (Mermaid and SVG).
- CASP and GRADE content, the Crossref adapter, and PRISMA diagrams.

### Phase 17: Grants and finance (Month 18)

**Goal:** Find funding, respond to it, and manage the money (HPP612).

**Tools:** PLAN-06 `search_grant_opportunities`, PLAN-07 `analyze_funding_announcement`, PLAN-08 `draft_grant_narrative`, PLAN-09 `build_program_budget`, PLAN-10 `economic_evaluation_calculator`, LEAD-01 `analyze_health_department_budget`, LEAD-02 `model_staffing_needs`.

**Engineering:**
- The Grants.gov adapter and a COMMBUYS listing snapshot.
- A NOFO/RFR parser that turns PDF or DOCX announcements into a checklist.
- Budget model exports to XLSX with live formulas, plus economic evaluation and staffing models.
- A "prepare a grant application" MCP prompt.

### Phase 18: Leadership, accreditation, and professional practice (Month 19)

**Goal:** Tools for running the department and meeting standards.

**Tools:** LEAD-03 `assess_foundational_public_health_services`, LEAD-04 `phab_accreditation_crosswalk`, LEAD-05 `build_strategic_plan`, LEAD-06 `assess_workforce_competencies`, LEAD-07 `board_of_health_meeting_assistant`, LEAD-08 `draft_standing_orders`, EBP-07 `qi_vs_research_determination`, POL-05 `public_health_ethics_analysis`, POL-06 `nursing_scope_of_practice_guide`.

**Engineering:**
- PHAB measures content, with a document crosswalk that reuses the Phase 1 index.
- SAPHE 2.0 and Foundational Public Health Services standards, and the competency frameworks.
- Open Meeting Law rules, standing order templates, 244 CMR content, OHRP decision logic, and ethics frameworks.

### Phase 19: Policy and law; v1.0 hardening (Month 20)

**Goal:** Policy tools, then a production-ready v1.0.

**Tools:** POL-01 `track_ma_legislation`, POL-02 `search_public_health_law`, POL-03 `monitor_federal_rules`, POL-04 `draft_policy_brief`, POL-07 `health_information_sharing_guide`.

**Engineering:**
- The Massachusetts Legislature data adapter and the Federal Register and Regulations.gov adapters.
- A consolidated law corpus (M.G.L. c.111, 105 CMR, 244 CMR, and local regulations from `/data`), and information-sharing decision logic.

**v1.0 hardening:**
- Threat model review, dependency and container scans, image size and startup time checks, and accessibility of generated documents.
- A user guide for nurses, an administrator guide for IT, and a final content review sweep.

## 8. Content maintenance after v1.0

Reference content is only useful if it is current. Each item has a `review_due` date, and CI fails when a review is overdue.

| Content | Typical update | Tools affected |
|---|---|---|
| ACIP/CDC schedules and DPH and professional-society recommendations | Annually, plus ad hoc changes | VAX-01, VAX-02, VAX-03 |
| CDC CDSi supporting data and test cases | With each schedule change | VAX-03 |
| Massachusetts school immunization requirements | Annually, before each school year | VAX-04 |
| DPH school immunization survey data | Annually | VAX-05 |
| 105 CMR, 244 CMR, M.G.L. | When amended | Most reference tools |
| Federal poverty guidelines and benefit limits | Annually | EQ-06, AGE-04 |
| ACS 5-year estimates | Annually | EQ-01–EQ-03, MCH-05, AGE-05 |
| CDC PLACES | Annually | CHR-01 |
| CDC SVI | About every two years | EQ-02 |
| Service directories (crisis, aging, naloxone, treatment) | Quarterly check | MH-01, MH-04, MH-05, AGE-02 |
| MassGIS boundaries | When MassGIS updates them | EPI-12, EPI-13 |

With one developer, plan to spend roughly 2–4 days a month on maintenance after v1.0.

## 9. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| PHI reaches the AI vendor through tool output | High | Aggregate-first design, automatic suppression, PHI detector, local mail-merge, and a check against the agency's AI-use policy before each pilot. |
| Errors in clinical or legal reference content | High | Citations and effective dates on every answer, review gates for the nurse, review-due dates enforced in CI, decision-support notices, and no automatic submission anywhere. |
| Federal, professional, and state vaccine recommendations keep diverging | High | VAX-02 shows each body's position with dates. A configurable default follows DPH. Recommendation content is reviewed whenever a body publishes a change. |
| Federal datasets are removed or changed (as happened to several in 2025) | Medium | Dated snapshots, archived copies, and CORE-08 data source status reporting. |
| Changes to Massachusetts web pages break snapshot builds | Medium | Checksums and diff alerts in the snapshot builder, and a manual-upload fallback for each source. |
| Government networks block or inspect traffic | Medium | Proxy and custom CA support, offline mode, and offline image tarballs. |
| Container runtime licensing or IT approval delays adoption | Medium | Raise it in Phase 0. Support Podman Desktop and Rancher Desktop as free alternatives. |
| Too many tools degrade the model's tool selection | Medium | Toolsets and role profiles and short precise tool descriptions are in place. A regression set of nurse prompts re-run at every release is planned and not yet built. |
| The CDSi forecaster or offline geocoder overruns | Medium | Each has a dedicated phase with lighter companion tools that can slip to the next phase. |
| Single-developer bus factor | Medium | Architecture decision records, strong test coverage, and contributor documentation from Phase 0. |
| License terms of "free" instruments (e.g., EPDS, C-SSRS) | Low | Record license terms in content metadata and confirm them during the nurse's content review. Move any instrument with restrictive terms to Appendix A. |

**Speeding up the plan:** A second developer could take the content-heavy phases (5, 7–11, 13, 18) while the first takes the adapter- and engine-heavy phases (2, 4, 6, 12, 14–17, 19). That would cut the calendar to about 11–12 months, but it doubles the nurse's review load.

---

## Appendix A: Tools that require outside resources

These 24 tools are not scheduled because none can be built with public resources alone. Each would take roughly 0.5–1 phase once its resource is in place.

**Resource types:**
- **License**: a purchased or licensed product, content, or service.
- **Access**: a data-access agreement or account for a state or agency system.
- **Professional**: sign-off by a qualified professional outside the nurse's role.
- **Privacy**: agency privacy and security approval plus a BAA with the AI vendor, because identifiable client data would reach the model.

| ID | Tool | Section | Outside resource needed | Type |
|---|---|---|---|---|
| VAX-10 | `query_miis_immunization_record` | Immunizations | MIIS data-access agreement with DPH, interface credentials, and agency approval to put client records into the AI context | Access + Privacy |
| VAX-11 | `send_immunization_reminders` | Immunizations | Messaging service (SMS, voice, email), consent and opt-out policy, and PHI approval | License + Privacy |
| CD-14 | `sync_maven_case` | Communicable disease | DPH authorization for MAVEN. There is no public API, so DPH would need to provide an interface. | Access |
| CD-15 | `query_syndromic_surveillance` | Communicable disease | NSSP ESSENCE account through DPH and a data use agreement | Access |
| CD-16 | `calculate_tb_medication_doses` | Communicable disease | Validation by a TB physician or clinical pharmacist, and medical director standing orders | Professional |
| CD-17 | `draft_isolation_quarantine_order` | Communicable disease | Legal templates and review from town counsel or DPH | Professional |
| CD-18 | `search_licensed_id_references` | Communicable disease | Licenses allowing software use of the APHA *Control of Communicable Diseases Manual* and AAP *Red Book* | License |
| SCH-09 | `sync_student_health_records` | School health | Vendor API access (for example SNAP Health Center, Aspen, PowerSchool), a district data-sharing agreement, and FERPA review | Access + Privacy |
| MCH-08 | `score_asq_developmental_screen` | Maternal and child health | ASQ-3 / ASQ:SE-2 license from the publisher | License |
| MCH-09 | `optimize_home_visit_routes` | Maternal and child health | Approval to put client addresses into the AI context (privacy approval and BAA), plus routing data or a routing service | Privacy |
| MCH-10 | `manage_client_caseload` | Maternal and child health | PHI approval, BAA, and security review of client data stored on the nurse's computer | Privacy |
| AGE-08 | `score_cognitive_screen` | Healthy aging | MoCA training and certification, an MMSE license, and permission for Mini-Cog use in software | License |
| AGE-09 | `review_older_adult_medications` | Healthy aging | AGS Beers Criteria license, a licensed drug knowledge base, and pharmacist review of the logic | License + Professional |
| EPR-12 | `publish_maps_to_arcgis` | Emergency preparedness | Esri ArcGIS Online organizational account | License |
| EPR-13 | `sync_eoc_situation_reports` | Emergency preparedness | License and account for the emergency operations platform (for example WebEOC) through MEMA or the HMCC | License + Access |
| EQ-12 | `send_closed_loop_referral` | Health equity | Contract or data-sharing agreement with Mass 211, Findhelp, or Unite Us, and PHI approval | License + Privacy |
| EPI-14 | `expert_determination_deidentification` | Epidemiology and data | A qualified statistician to perform and certify the HIPAA Expert Determination | Professional |
| EPI-15 | `query_public_health_data_warehouse` | Epidemiology and data | DPH Public Health Data Warehouse application, approval, and secure analysis environment | Access |
| LEAD-09 | `cpt_billing_assistant` | Leadership and finance | AMA CPT license, billing and compliance expertise, and payer contracts | License + Professional |
| POL-08 | `ana_phn_scope_standards_reference` | Policy and ethics | ANA permission or license to embed and search the publication | License |
| COMM-06 | `certified_translation_workflow` | Health communication | Contract with a qualified translation or interpreter vendor | License |
| EBP-08 | `search_cinahl` | Evidence-based practice | EBSCO CINAHL subscription with API access | License |
| CORE-10 | `check_drug_interactions` | Platform | Licensed drug-interaction knowledge base (NLM retired its free interaction API in 2024) | License |
| CORE-11 | `search_licensed_clinical_reference` | Platform | Enterprise license with API access for UpToDate, Lexicomp, or similar | License |

**Suggested priority once resources are available:**
1. **A BAA with the AI vendor and an agency privacy approval.** This one step unblocks four Privacy tools: MCH-09, MCH-10, VAX-11, and EQ-12.
2. **MIIS access (VAX-10).** It pairs directly with the VAX-03 forecaster and removes manual dose entry.
3. **MAVEN and syndromic surveillance access (CD-14, CD-15).** These connect the Phase 3 communicable disease tools to the state systems.
4. **Licensed references (CD-18, CORE-10, CORE-11, EBP-08).** These are useful but have free partial substitutes already in the plan.
