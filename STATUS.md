# Project status and handoff

A handoff document for whoever picks up this project next, human or AI agent. Read it before
starting work, and **update it before you finish** (see [Keeping this file current](#keeping-this-file-current)).

- **Last updated:** 2026-11-08
- **Current version:** 0.7.0 (Phases 0–6 of [`project_plan.md`](project_plan.md)); not yet released
- **Next phase:** Phase 7, school health (then Pilot 2). Pilot 1 (after Phase 3) waits on the
  nurse's review of the communicable-disease content.

## 1. Where things stand

| Phase | Theme | State | Where |
|---|---|---|---|
| — | Tool brainstorm and project plan | Merged | PR #1 |
| 0–1 | Foundation; local data, privacy guardrails, documents (11 tools) | Merged; awaiting acceptance | PR #2 |
| 2 | Epidemiology and biostatistics engine (8 tools) | Code complete; awaiting acceptance | PR #3 |
| 3 | Communicable disease I (8 tools + outbreak prompt) | Code complete; content review needed before Pilot 1 | PR #3 |
| 4 | Public data and evidence platform (8 tools) | Code complete; awaiting acceptance | PR #3 |
| 5 | Immunizations I (9 tools) | Code complete; content review needed | PR #3 |
| 6 | Immunization forecasting and surveillance feeds (3 tools) | Code complete; CDSi exceptions to review | PR #3 |
| 7–19 | See the roadmap in `project_plan.md` §6 | Not started | — |

PR #3 (branch `ccr-58e7099a-2x8idl`) holds Phases 2–6 plus a re-check of the Phase 0–1 content.
If it has been merged, start new work on a fresh branch from `main`. Phases 3–6 were built in
parallel by helper agents on per-phase branches and merged into that branch.

### Tools implemented (47 of 148)

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
| `check_document_accessibility` | COMM-05 | comms | 1 | `crates/phn-tools/src/comms.rs` |
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

MCP wrappers live in `crates/phn-server/src/server.rs` (Phase 0) and `phase1.rs`–`phase6.rs`;
the outbreak MCP prompt is in `prompts.rs`. The registry (tool → plan ID → toolset → phase) is
`crates/phn-tools/src/registry/` with one file per phase.

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
| Every phase: acceptance script run by the nurse | **Waiting on the nurse** (`docs/acceptance/phase-*.md`) |

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
   - `content/data_sources.yaml`.
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
3. **Run the acceptance scripts** for Phases 0–6 on Windows and macOS and send the IT request.
4. **Publish a release:** after PR #3 merges, tag `v0.7.0` to run `.github/workflows/release.yml`
   (GHCR image, SBOM, Trivy scan, cosign signature, offline tarballs). No release has ever been
   tagged; the workflow has never run, so watch it closely the first time.
5. **Optional API keys** for nurses' installations: `PHN_CENSUS_API_KEY` (needed for live ACS
   from some networks), `PHN_NCBI_API_KEY`, `PHN_OPENFDA_API_KEY`. Everything works without them
   (snapshots or lower rate limits).

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
- GitHub warns that `actions/checkout@v4` runs on a deprecated Node.js version; bump to v5.

## 3. How the code is organized

```
crates/
  phn-core     config (env + /cache/phn-settings.toml), Envelope, toolsets/profiles, errors, fuzzy text
  phn-content  embedded YAML content + JSON Schemas (content/), reportable conditions, source registry
  phn-data     HTTP client, SQLite cache, live→cache→snapshot fetch, snapshot store, MA reference lists,
               adapters: Socrata (data.cdc.gov), Census ACS, SVI, PLACES, NCBI E-utilities, CDC feeds,
               boundaries, openFDA/DailyMed/RxNorm, CVX/MVX, NNDSS, respiratory and wastewater
  phn-epi      rates, suppression, standardization, 2×2/Mantel–Haenszel, GLMs (IRLS), trends/joinpoint,
               hypothesis tests, SPC, sample size, aberration detection (EARS, CUSUM, Farrington)
  phn-cdsi     CDC CDSi supporting data (XML) and the immunization evaluation/forecasting engine
  phn-local    /data sandbox, parsers, PHI detector, de-identification, profiling, tantivy policy index
  phn-docs     document model, DOCX/PDF/XLSX/CSV/SVG/PNG writers, mail-merge, a11y checks,
               chart renderer and validated palette (ADR 0006)
  phn-tools    one function per tool; localdata (guardrails for analyzing local files); charts
  phn-server   rmcp server: per-phase #[tool] routers, prompts, toolset filtering, resources, main.rs
content/       reference content (YAML) with review metadata; schemas in content/schemas/
snapshots/     dated data snapshots + manifest.json with SHA-256 checksums (about 6 MB, 15 snapshots)
fixtures/      synthetic test corpus, all fictional; seeded_identifiers.json lists every planted identifier
docs/          tools.md, adr/ (0001–0007), acceptance/ (per phase), cdsi-test-results.md
xtask/         build-snapshots, build-public-snapshots, build-surveillance-snapshots,
               build-guide-corpus, build-name-lists, validate-content; scripts/ (Python builders)
```

Read these before changing architecture: `project_plan.md` §5, `docs/adr/0001`–`0007`, and
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
  `crates/phn-server/tests/mcp_integration.rs`.
- **Clinical and legal answers** cite sources, state review status, and call `.decision_support()`.
- **Vaccine recommendations** default to DPH, then AAP/ACOG/AAFP, then CDC/ACIP, and always show
  each body's position (ADR 0007).
- **Content changes** must keep `cargo xtask validate-content` green. Do not mark content
  `reviewed` yourself; only the subject matter expert can.
- **Stdout is the MCP protocol.** Log to stderr only (`tracing`).

## 4. Working on the project

### Commands

```sh
cargo test --workspace                                   # 337 tests: unit, golden, CDSi cases, MCP integration
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
- **Disk:** the sandbox has a fixed allowance; `target/debug/incremental` grows to several GB.
  Delete it when space runs low.
- **Library APIs** (read the source in `~/.cargo/registry/src/` before guessing):
  - `rmcp` 3.9: `output_schema = schema_for_output::<Envelope<T>>()`; per-phase
    `#[tool_router(router = phaseN_router, vis = "pub(crate)")]` blocks merged in
    `PhnServer::new` (`allow_empty` for a router with no tools yet); prompts via
    `list_prompts`/`get_prompt` with `enable_prompts()`.
  - `quick-xml` 0.42, `tantivy` 0.27 (`en_stem` tokenizer improves recall), `reqwest` 0.13,
    `skrifa` as before. rustfmt removes the blank line before a comment marker in `vec!`/array
    literals.

## 5. Suggested next steps (Phase 7: school health → Pilot 2)

From `project_plan.md`: SCH-01 `get_ma_school_screening_requirements`, SCH-02
`track_school_screenings`, SCH-03 `school_medication_administration_guide`, SCH-04
`build_individualized_health_plan`, SCH-05 `concussion_return_to_learn_tracker`, SCH-06
`monitor_school_absenteeism`, SCH-07 `build_school_health_annual_report`, SCH-08
`child_care_health_consultation_checklist`.

- Reuse: the Phase 5 school-requirement rule engine and roster pattern (batch results to
  `/data/output`, aggregates back), the Phase 3 aberration detection (`phn_epi::aberration`) for
  absenteeism spikes, `phn_tools::localdata`, charts, and `export_document` mail-merge for
  multilingual referral letters.
- Content: 105 CMR 200/201/210 and 606 CMR 7.00, fetched from mass.gov, all `pending`.
- Before Pilot 1 and Pilot 2, get the nurse's content reviews done (section 2).

## 6. History

| Date | Change | PR |
|---|---|---|
| 2026-11-07 | Tool brainstorm (172 tools) and phased project plan | #1 |
| 2026-11-07 | Phases 0 and 1: workspace, 11 tools, privacy guardrails, Docker, CI, docs | #2 |
| 2026-11-07 | Fixed CI clippy failure on Rust 1.99 (`as_chunks`); added STATUS.md and CLAUDE.md | #2 |
| 2026-11-08 | Phases 2–6: 36 tools, `phn-epi` engines, `phn-cdsi`, data adapters and snapshots; Phase 0–1 content re-checked against mass.gov | #3 |

## Keeping this file current

At the end of every working session, update:

1. **Last updated**, version, and next phase at the top.
2. **Where things stand** and the tool table.
3. **Open items**: add new debt, and remove items that are resolved.
4. **History**: one line per pull request.

Keep it factual. If you were not able to verify something, say so.
