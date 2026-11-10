# Contributing

Thank you for helping. This project is built in phases (see [`project_plan.md`](project_plan.md));
please keep changes within the current phase unless discussed first.

## Setup

Install Rust (stable, 1.88 or later). Then:

```sh
cargo test --workspace
cargo +stable clippy --workspace --all-targets -- -D warnings   # CI uses the latest stable clippy
cargo fmt --all
cargo xtask validate-content
```

CI also runs `cargo deny check`, `cargo audit`, and a Docker build with a smoke test.

## Adding a tool

A tool is done when it meets the plan's definition of done:

1. **Logic** in `crates/phn-tools/src/<theme>/`: a function `fn tool(ctx: &Context, args: Args)
   -> Result<Envelope<Data>>`. `Args` and `Data` derive `Deserialize`/`Serialize` and
   `JsonSchema`; document every field (the doc comments become the schema descriptions the
   model reads).
2. **Registration** in `crates/phn-tools/src/registry/` (plan ID, toolset, phase), in the file
   for the tool's phase (for example `phase14.rs`, listed in `registry/mod.rs`), and an
   `#[tool]` wrapper in `crates/phn-server/src/` with a description written for the model,
   annotations, and `output_schema = schema_for_output::<Envelope<Data>>()`. File and CPU heavy
   tools run through `blocking(...)`.
3. **Envelope:** a concise markdown summary; every source cited with its date and mode; caveats
   for small numbers, data limits, and review status; an accurate privacy block; and
   `.decision_support()` for clinical or legal answers.
4. **Privacy:** tools that read local data return aggregates by default, apply suppression, and
   write anything identifiable to the output folder. Add the tool to the leak test in
   `crates/phn-server/tests/mcp_integration.rs`.
5. **Tests:** unit tests, golden tests for calculations (reference values from R, Epi Info, or
   SciPy committed as fixtures), and MCP integration tests (the schema test covers new tools
   automatically).
6. **Docs:** a section in [`docs/tools.md`](docs/tools.md) (a test checks every tool is listed),
   an entry in the user guide's [`guide-content.js`](docs/user-guide/guide-content.js) with the
   guide rebuilt (see [`docs/user-guide/README.md`](docs/user-guide/README.md); a test checks
   this too), and a line in [`CHANGELOG.md`](CHANGELOG.md).

## Reference content and review

Content lives in `content/` as YAML with a JSON Schema in `content/schemas/`. Each file has a
`meta.review` block:

```yaml
review:
  status: pending        # or reviewed
  reviewed_by: null      # name and credentials of the reviewer
  reviewed_on: null      # YYYY-MM-DD
  review_due: 2027-02-01 # CI fails after this date
```

When the subject matter expert has checked an item against its sources, set `status: reviewed`,
fill in `reviewed_by` and `reviewed_on`, set the next `review_due`, bump `version`, and remove
any `confidence: low` markers that were resolved. `cargo xtask validate-content` enforces the
schema and fails when a review is overdue.

## Data snapshots

`cargo xtask build-snapshots` rebuilds the MassGIS reference lists in `snapshots/` (towns,
counties, HMCC regions, school districts) and updates the manifest (sources, vintage, SHA-256
checksums). `--check` compares without writing and is run weekly in CI.

The other snapshots have their own builders: `cargo xtask build-public-snapshots` (ACS, SVI,
PLACES, boundaries), `build-surveillance-snapshots` (NNDSS, respiratory, wastewater),
`build-guide-corpus` (DPH guide and fact sheets), `build-service-directories`,
`build-nursing-home-snapshot`, `build-older-adult-snapshots`, and the Python scripts in
`xtask/scripts/`. Each entry in `snapshots/manifest.json` says how it was built in its
`derivation` field. `cargo xtask build-name-lists` rebuilds the PHI detector's name lists.

Review the diff before committing a refreshed snapshot, and run `cargo xtask validate-content`,
which checks the snapshot checksums.

## Architecture decisions

Significant decisions are recorded in [`docs/adr/`](docs/adr/). Add a new record rather than
editing an accepted one.
