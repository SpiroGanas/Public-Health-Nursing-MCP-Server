# Contributing

Thank you for helping. This project is built in phases (see [`project_plan.md`](project_plan.md));
please keep changes within the current phase unless discussed first.

## Setup

Install Rust (stable, 1.88 or later). Then:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
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
2. **Registration** in `crates/phn-tools/src/registry.rs` (plan ID, toolset, phase) and an
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
6. **Docs:** a section in [`docs/tools.md`](docs/tools.md) (a test checks every tool is listed)
   and a line in [`CHANGELOG.md`](CHANGELOG.md).

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

`cargo xtask build-snapshots` rebuilds `snapshots/` from MassGIS and updates the manifest
(sources, vintage, SHA-256 checksums). `--check` compares without writing and is run weekly in
CI. Review the diff before committing a refreshed snapshot.

## Architecture decisions

Significant decisions are recorded in [`docs/adr/`](docs/adr/). Add a new record rather than
editing an accepted one.
