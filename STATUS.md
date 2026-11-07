# Project status and handoff

A handoff document for whoever picks up this project next, human or AI agent. Read it before
starting work, and **update it before you finish** (see [Keeping this file current](#keeping-this-file-current)).

- **Last updated:** 2026-11-07
- **Current version:** 0.2.0 (Phases 0 and 1 of [`project_plan.md`](project_plan.md))
- **Next phase:** Phase 2, the epidemiology and biostatistics engine

## 1. Where things stand

| Phase | Theme | State | Where |
|---|---|---|---|
| — | Tool brainstorm and project plan | Merged | PR #1 (`brainstorming.md`, `project_plan.md`) |
| 0 | Foundation (4 tools) | Code complete; awaiting human acceptance | PR #2 |
| 1 | Local data, privacy guardrails, documents (7 tools) | Code complete; awaiting human acceptance | PR #2 |
| 2 | Epidemiology and biostatistics engine (8 tools) | Not started | — |
| 3–19 | See the roadmap in `project_plan.md` §6 | Not started | — |

PR #2 (branch `ccr-64141681-6yjgf6`) holds all of Phases 0 and 1. If it has been merged, start
new work on a fresh branch from `main`.

### Tools implemented (11 of 148)

| Tool | Plan ID | Toolset | Code |
|---|---|---|---|
| `get_data_source_status` | CORE-08 | core | `crates/phn-tools/src/core/status.rs` |
| `configure_jurisdiction` | CORE-09 | core | `crates/phn-tools/src/core/configure.rs` |
| `read_local_file` | CORE-04 | core | `crates/phn-tools/src/core/files.rs` |
| `search_agency_policies` | CORE-05 | core | `crates/phn-tools/src/core/policies.rs` |
| `export_document` | CORE-06 | core | `crates/phn-tools/src/core/export.rs` |
| `calculate_rates` | EPI-01 | epi | `crates/phn-tools/src/epi/rates.rs` |
| `apply_small_cell_suppression` | EPI-06 | epi | `crates/phn-tools/src/epi/suppression.rs` |
| `profile_local_dataset` | EPI-07 | epi | `crates/phn-tools/src/epi/profile.rs` |
| `deidentify_dataset` | EPI-08 | epi | `crates/phn-tools/src/epi/deidentify.rs` |
| `lookup_reportable_condition` | CD-01 | cd | `crates/phn-tools/src/cd.rs` |
| `check_document_accessibility` | COMM-05 | comms | `crates/phn-tools/src/comms.rs` |

MCP wrappers live in `crates/phn-server/src/server.rs` (Phase 0) and
`crates/phn-server/src/phase1.rs` (Phase 1). The registry of tool → plan ID → toolset is
`crates/phn-tools/src/registry.rs`.

### Exit criteria

| Criterion | State |
|---|---|
| Phase 1: PHI detector flags every seeded identifier in the synthetic corpus | Done (automated test) |
| Phase 1: no row-level data returned by default | Done (automated leak test) |
| Phase 1: exported DOCX passes the server's own accessibility checks | Done (automated; PDF too) |
| Phase 0: the nurse runs the tools from Claude Desktop on Windows and a Mac | **Waiting on the nurse** (`docs/acceptance/phase-0-1.md`) |
| Phase 0: agency IT asked about container runtime approval | **Waiting on the nurse** (template in the acceptance script) |

## 2. Open items

### Needs a person (do not attempt to complete these yourself)

1. **Review the reportable-condition content.** `content/cd/reportable_conditions.yaml` was
   drafted from memory because mass.gov was unreachable from the development environment. It is
   marked `status: pending`, each entry has a `confidence` level, and tool answers say it is a
   draft. The subject matter expert (the DNP-prepared public health nurse who owns the product)
   must check every entry against the current 105 CMR 300.000 before Pilot 1. **Review is due
   2027-02-01; CI starts failing after that date** until `meta.review` is updated.
2. **Review the data source registry** (`content/data_sources.yaml`), also pending, same due date.
3. **Run the acceptance script** on Windows and macOS and send the IT request.
4. **Publish the first release:** after PR #2 merges, tag `v0.2.0` to run
   `.github/workflows/release.yml` (GHCR image, SBOM, Trivy scan, cosign signature, offline
   tarballs). That workflow has never run; watch it closely the first time.

### Known limitations and technical debt

- **HMCC regions** are derived from the 2005 MassGIS DPH preparedness-region layer, with Regions
  4A and 4B merged into 4AB. They have not been checked against DPH's current HMCC membership.
- **School districts** come from a 2019 MassGIS layer; newer regionalizations are missing.
- **PDF output** is single-column US Letter, with no kerning, ligatures, or italic face. Scripts
  outside DejaVu Sans (Chinese, Khmer, Arabic) show as `?` in PDF/SVG/PNG; the tool warns and
  recommends DOCX for those.
- **No OCR:** scanned PDFs return no text; the tools report which pages.
- **PHI detection is heuristic.** It errs toward flagging and is tested against a synthetic
  corpus, but it can miss identifiers. Tool responses say so.
- **The policy index** rebuilds completely when any file changes. That is fine for a folder of
  policies, but slow for thousands of files.
- **Live data adapters** do not exist yet. The live → cache → snapshot chain
  (`crates/phn-data/src/fetch.rs`) is built and tested, but only `get_data_source_status` makes
  network calls so far. The first real adapters arrive in Phase 4.
- **MCP prompts** (for example, "investigate an outbreak") are not implemented; the first is
  planned for Phase 3.
- GitHub warns that `actions/checkout@v4` runs on a deprecated Node.js version. It still works;
  bump it to v5 when convenient.

## 3. How the code is organized

```
crates/
  phn-core     config (env + /cache/phn-settings.toml), Envelope, toolsets/profiles, errors, fuzzy text
  phn-content  embedded YAML content + JSON Schemas (content/), reportable conditions, source registry
  phn-data     HTTP client (proxy + CA), SQLite cache, fetch fallback, snapshot store, MA reference lists
  phn-epi      rates with exact Poisson CIs, small-cell suppression     <- Phase 2 extends this
  phn-local    /data sandbox, parsers, PHI detector, de-identification, profiling, tantivy policy index
  phn-docs     document model (Markdown), DOCX/PDF/XLSX/CSV/SVG/PNG writers, mail-merge, a11y checks
  phn-tools    one function per tool: fn(&Context, Args) -> Result<Envelope<Data>>
  phn-server   rmcp server: #[tool] wrappers, toolset filtering, resources, main.rs (stdio, `check`)
content/       reference content (YAML) with review metadata; schemas in content/schemas/
snapshots/     MassGIS-derived CSVs + manifest.json with SHA-256 checksums (built by xtask)
fixtures/      synthetic test corpus, all fictional; seeded_identifiers.json lists every planted identifier
docs/          tools.md (tool reference), adr/ (decisions), acceptance/ (nurse acceptance scripts)
xtask/         build-snapshots, build-name-lists, validate-content
```

Read these before changing architecture: `project_plan.md` §5 (architecture),
`docs/adr/0001`–`0005`, and `CONTRIBUTING.md` (definition of done for a tool).

### Design rules that must not be broken

- **Every tool returns `phn_core::Envelope<T>`**, with a markdown summary, typed data, sources
  (with dates and mode), caveats, and a privacy block. Problems the nurse can fix are tool-level
  errors (`isError: true`), not protocol errors.
- **Privacy:** never return row-level local data by default; masked rows only with an explicit
  argument; never offer a way to turn redaction off; identifiable output goes to `/data/output`
  only; frequency tables go through suppression. Any new data tool must be added to the leak
  test in `crates/phn-server/tests/mcp_integration.rs`.
- **Clinical and legal answers** cite sources, state review status, and call `.decision_support()`.
- **Content changes** must keep `cargo xtask validate-content` green. Do not mark content
  `reviewed` yourself; only the subject matter expert can.
- **Stdout is the MCP protocol.** Log to stderr only (`tracing`).

## 4. Working on the project

### Commands

```sh
cargo test --workspace                                   # 99 tests: unit, golden, MCP integration, stdio binary
cargo +stable clippy --workspace --all-targets -- -D warnings   # match CI's toolchain (see below)
cargo fmt --all
cargo xtask validate-content                             # schemas, review dates, snapshot checksums
cargo xtask build-snapshots --check                      # rebuild MassGIS snapshots and diff (needs network)
cargo deny check && cargo audit                          # licenses, advisories, sources
cargo run -p phn-server --bin phn-mcp-server -- check    # needs PHN_DATA_DIR / PHN_CACHE_DIR set
docker build -t phn-mcp-server:dev .                     # distroless image, about 96 MB
```

### Toolchain

CI uses the latest stable Rust (1.99 as of this writing). Newer clippy releases add lints, and
one of those has already broken CI once, so run clippy with `cargo +stable` (after
`rustup toolchain install stable`) before pushing. The MSRV is 1.88 (`rust-version` in
`Cargo.toml`).

### Lessons from the first sessions

- **Disk:** debug builds once filled the sandbox (23 GB of `target/`). The dev profile now uses
  line-table debug info only (about 2 GB). Clean `target/` and Docker build cache if space gets low.
- **Network in sandboxes:** mass.gov, census.gov, cdc.gov, and law.cornell.edu were blocked;
  crates.io, PyPI, GitHub, and the MassGIS S3 bucket worked. Do not invent regulatory content
  you cannot verify: mark it `pending` with a confidence level and tell the user.
- **Library APIs** used here are newer than many models' training data. Read the crate source in
  `~/.cargo/registry/src/` before guessing:
  - `rmcp` 3.9: set `output_schema = schema_for_output::<Envelope<T>>()` on each `#[tool]`. A
    second `#[tool_router(router = phase1_router, vis = "pub(crate)")]` block is merged in
    `PhnServer::new`. `Peer<RoleServer>` can be a tool parameter (used to send
    `tools/list_changed`).
  - `quick-xml` 0.42: entity references arrive as separate `Event::GeneralRef` events;
    `local_name().as_ref()` is `&str`; attribute values use `normalized_value(XmlVersion::Implicit1_0)`.
  - `tantivy` 0.27: `TopDocs::with_limit(n).order_by_score()`.
  - `reqwest` 0.13: add an agency CA with `tls_certs_merge`.
  - `skrifa` replaced `ttf-parser`, which is unmaintained (RUSTSEC-2026-0192). `subsetter`
    renumbers glyphs, so PDF text must use the new glyph IDs.
- **Docker in a sandbox:** `dockerd` can be started manually. Builds behind a TLS-inspecting proxy
  need the proxy CA in the build stage only. Use a local Dockerfile variant and never commit the CA.

## 5. Suggested next steps (Phase 2)

From `project_plan.md`: EPI-02 `age_adjust_rates`, EPI-03 `analyze_two_by_two`, EPI-04
`calculate_sample_size`, EPI-05 `compute_standardized_ratio`, EPI-09 `analyze_trends`, EPI-10
`run_statistical_test`, EPI-11 `create_chart`, EBP-06 `create_spc_chart`.

- Extend `crates/phn-epi`: gamma-method CIs and the 2000 US standard population (an NCHS
  table); 2×2 measures with exact tests; GLM fitting by IRLS (Poisson, negative binomial,
  logistic); segmented regression; SPC rules.
- Charts: add `plotters`, with an accessible palette and generated alt text. Charts should embed
  through `export_document`, whose image support already exists.
- **Golden tests are the exit criterion:** generate reference values with R, Epi Info, or SciPy
  and commit them as fixtures, following `crates/phn-epi/tests/golden.rs`. Note in the fixture
  how each value was produced.
- Each new tool: follow the checklist in `CONTRIBUTING.md`, add it to `registry.rs`,
  `docs/tools.md` (a test enforces this), the README tool table, and `CHANGELOG.md`.

## 6. History

| Date | Change | PR |
|---|---|---|
| 2026-11-07 | Tool brainstorm (172 tools) and phased project plan | #1 |
| 2026-11-07 | Phases 0 and 1: workspace, 11 tools, privacy guardrails, Docker, CI, docs | #2 |
| 2026-11-07 | Fixed CI clippy failure on Rust 1.99 (`as_chunks`); added STATUS.md and CLAUDE.md | #2 |

## Keeping this file current

At the end of every working session, update:

1. **Last updated**, version, and next phase at the top.
2. **Where things stand** and the tool table.
3. **Open items**: add new debt, and remove items that are resolved.
4. **History**: one line per pull request.

Keep it factual. If you were not able to verify something, say so.
