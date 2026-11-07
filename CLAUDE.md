# CLAUDE.md

Instructions for AI agents working in this repository.

## Start here

1. Read [`STATUS.md`](STATUS.md). It records what is done, what is open, and what was learned in
   earlier sessions. Then read the parts of [`project_plan.md`](project_plan.md) and
   [`CONTRIBUTING.md`](CONTRIBUTING.md) that apply to your task.
2. Before you finish, update `STATUS.md` (see its last section) and include that change in your
   pull request.

## Clarify the request first

If a request is ambiguous, underspecified, or could reasonably be read more than one way, **ask
up to 10 clarifying questions before starting work**. Ask only the questions whose answers would
change what you build. Do not ask about things you can find in the code, the plan, or these
documents, or that have a sensible default. Ask them all in one round where you can; the
question tool takes about four questions per call, so use several calls if you need more. Offer
choices with a recommended option. If the request is already clear, do not ask; proceed.

## Always work through a pull request and watch it

For every change:

1. Work on a branch, never directly on `main`. If an open pull request already covers the work,
   add to it. Otherwise create a branch from the latest `main`.
2. Run the checks below, commit with a clear message, and push.
3. **Open a pull request** (unless one is already open for the branch). Use a description that
   covers the summary, what changed, testing, and anything needing human review.
4. **Watch the pull request:** subscribe to its activity, such as with `subscribe_pr_activity`.
   Fix CI failures, address review comments, and keep it green and mergeable until it is merged
   or closed. Do not merge or approve it yourself.

## Checks before every push

```sh
cargo fmt --all
cargo +stable clippy --workspace --all-targets -- -D warnings   # CI uses the latest stable clippy
cargo test --workspace
cargo xtask validate-content
```

Run `cargo deny check` when you change dependencies.

## Rules that must not be broken

- **Privacy first.** Everything a tool returns is sent to the AI vendor. Never return row-level
  local data by default, never add a way to turn off redaction, and write identifiable output
  only to `/data/output`. Add every new data tool to the leak test in
  `crates/phn-server/tests/mcp_integration.rs`.
- **Every tool returns `phn_core::Envelope<T>`**, with sources (with dates), caveats, and an
  accurate privacy block. Clinical and legal answers are decision support only.
- **Do not invent regulatory or clinical content.** If you cannot verify it against its source,
  mark it `status: pending` with a confidence level and tell the user. Only the subject matter
  expert may mark content `reviewed`.
- **Stdout carries the MCP protocol.** Log to stderr only.
- Never skip, disable, or weaken a test to get CI green.

## Useful facts

- Rust workspace: `crates/phn-*` plus `xtask`. Tool logic lives in `crates/phn-tools`; the MCP
  wrappers live in `crates/phn-server`.
- Several dependencies (`rmcp` 3.9, `quick-xml` 0.42, `tantivy` 0.27, `reqwest` 0.13) have newer
  APIs than many models know. Read their source under `~/.cargo/registry/src/` before guessing.
  `STATUS.md` lists the pitfalls already found.
- The user is a DNP-prepared public health nurse in Massachusetts. Write for that expert: plain
  language, with methods and sources shown.
