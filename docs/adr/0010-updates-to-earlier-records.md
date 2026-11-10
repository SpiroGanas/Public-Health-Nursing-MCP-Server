# ADR 0010: Updates to earlier records, and the container's time zone

- Status: accepted
- Date: 2026-11-10
- Phase: peer review of Phases 0–13

## Context

Accepted records are not edited (see `CONTRIBUTING.md`). A peer review of Phases 0–13 compared
the records with the code and found three statements that the project has since moved past, in
ADR 0001, ADR 0004, and ADR 0008. This record supersedes those statements and leaves the rest of
each record in force. The review also led to one new decision, about the time zone the server
runs in.

## Decision

### 1. ADR 0001: the workspace has a ninth crate, `phn-cdsi`

ADR 0001 says the Cargo workspace mirrors the plan's crate layout. Phase 6 added a crate the
plan did not list: `crates/phn-cdsi`, which holds CDC's Clinical Decision Support for
Immunization (CDSi) supporting data and the engine that evaluates dose histories and forecasts
doses due. It depends on no other crate in the workspace, and its tests run CDC's published
test cases (`docs/cdsi-test-results.md`). `project_plan.md` section 5.1 now lists it.

### 2. ADR 0004: the number of seeded identifiers

ADR 0004 says the synthetic corpus has 266 seeded identifiers. That was the count at Phase 1.
Each later phase added fictional files, and as of version 0.14.0
`fixtures/synthetic/seeded_identifiers.json` lists 1,479. The rule is unchanged: no tool may
return any value in that file, and the leak test in `crates/phn-server/tests/mcp_integration.rs`
checks it. Documents should point to the file instead of quoting a count.

### 3. ADR 0008: translated templates are found by more than one piece of code

ADR 0008 names `crates/phn-tools/src/school/templates.rs` as the shared code for agency
translations in `/data/templates/<language>/`, and gives `haitian-creole` as the folder name for
Haitian Creole. Later phases did not all reuse that code. As of this review the folder is looked
up in six places, and they do not match folder names in the same way:

| Code | Used by | Files | How the folder name is matched |
|---|---|---|---|
| `school/templates.rs` (`TemplateResolver`) | School letters and plans (SCH-02, SCH-04, SCH-05); crisis messages (EPR-10) | `<kind>.md`, `.txt`, or `.markdown` | Language names and codes from its own table. Spaces, underscores, and hyphens are treated alike, so `haitian_creole`, `haitian-creole`, and `ht` all work. |
| `mch/content.rs` | Lead and newborn family teaching sheets (MCH-04, MCH-01) | `.docx` (used as is) or `.md` | The language with spaces written as underscores, plus its code or name: `haitian_creole` or `ht`. The hyphenated spelling is being added in this review. |
| `aging/localfill.rs` | Protective services narratives and health care proxies (AGE-03, AGE-07) | `<template>.md` | Names and codes from its own table; spaces, underscores, and hyphens are treated alike. |
| `core/practice.rs` | LTBI patient calendar (CD-12); newcomer checklist (EQ-11) | `ltbi_calendar.md`, `newcomer_checklist.md` | The language as the nurse gives it, in lower case with spaces written as underscores (`haitian_creole`). Codes and names are not matched to each other. |
| `epr/shelter.rs` | Shelter intake forms (EPR-03) | `cmist_intake.md` | The language exactly as written in the registration file, in lower case. A two-word language keeps its space (`haitian creole`). Codes and names are not matched to each other. |
| `comms/translation.rs` | Translation workflow (COMM-02), which reports what each folder holds | any file | Names, codes, and aliases from its own table. |

Decisions:

- The folder spelling to give agencies is the language name in lower case with an underscore
  between words: `spanish`, `portuguese`, `haitian_creole`. `haitian-creole` and the code `ht`
  are also accepted by the school, crisis message, and teaching sheet tools. `docs/tools.md`
  says this, and says where a tool is stricter.
- The shelter intake tool is the exception today: it needs the folder to match the file's
  language value, space included. `docs/tools.md` states that under the tool.
- New tools that read `/data/templates/` use `TemplateResolver`. Moving the other four lookups
  onto it, so that one folder serves every tool, is follow-up work and is not done in this
  review.
- Everything else in ADR 0008 stands: English templates only in the server, translations
  supplied and approved by the agency, a visible count when a family's language falls back to
  English, and no machine translation.

### 4. New decision: the container runs on Eastern time

A container with no time zone set runs on UTC. Tools that default a date to "today" (for
example an assessment date, a deadline, or the school year) would then move to the next day at
7 p.m. Eastern standard time, or 8 p.m. daylight time, and the dates and times printed on
documents would be UTC.

- The image sets `TZ=America/New_York`, so "today" and document dates and times are Eastern
  time, the time zone of every Massachusetts board of health.
- A nurse or IT department can choose another zone by adding `-e TZ=...` to the client
  configuration (for example `-e TZ=America/Chicago`). The README lists `TZ` with the other
  environment variables.
- `phn-mcp-server check` prints the local time and its offset, so the setting can be confirmed.

## Consequences

- An agency that keeps one folder per language should name two-word languages with an
  underscore. Until the lookups are unified, shelter intake forms in a two-word language need a
  second folder spelled as the registration file spells the language.
- Run outside the container (for example `cargo run` during development), the server uses the
  computer's own time zone.
- Dates the nurse gives explicitly are not affected by the time zone; only defaults and printed
  timestamps are.
