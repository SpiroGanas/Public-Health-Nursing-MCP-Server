# ADR 0010: Updates to earlier records, the container's time zone, and roster name matching

- Status: accepted
- Date: 2026-11-10 (sections 3 and 5 updated 2026-11-11)
- Phase: peer review of Phases 0–13

## Context

Accepted records are not edited (see `CONTRIBUTING.md`). A peer review of Phases 0–13 compared
the records with the code and found three statements that the project has since moved past, in
ADR 0001, ADR 0004, and ADR 0008. This record supersedes those statements and leaves the rest of
each record in force. The review also led to two new decisions: the time zone the server runs
in, and how staff contacts are matched to names in a roster.

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
up in five places, and they do not match folder names in the same way:

| Code | Used by | Files | How the folder name is matched |
|---|---|---|---|
| `school/templates.rs` (`TemplateResolver`) | School letters and plans (SCH-02, SCH-04, SCH-05); crisis messages (EPR-10) | `<kind>.md`, `.txt`, or `.markdown` | Language names and codes from its own table. Spaces, underscores, and hyphens are treated alike, so `haitian_creole`, `haitian-creole`, and `ht` all work. |
| `mch/content.rs` | Lead and newborn family teaching sheets (MCH-04, MCH-01) | `.docx` (used as is) or `.md` | The language with spaces written as underscores, plus its code or name, and the same names with hyphens: `haitian_creole`, `haitian-creole`, or `ht`. |
| `aging/localfill.rs` | Protective services narratives and health care proxies (AGE-03, AGE-07) | `<template>.md` | Names and codes from its own table; spaces, underscores, and hyphens are treated alike. |
| `core/practice.rs` (`language_folder`) | LTBI patient calendar (CD-12); newcomer checklist (EQ-11); shelter intake forms (EPR-03), from the language column of the registration file | `ltbi_calendar.md`, `newcomer_checklist.md`, `cmist_intake.md` | The language as given, in lower case, with spaces written as underscores and every character other than ASCII letters, digits, `-`, and `_` dropped ("Haitian Creole" → `haitian_creole`). Codes and names are not matched to each other: a value "ht" needs a folder named `ht`. |
| `comms/translation.rs` | Translation workflow (COMM-02), which reports what each folder holds | any file | Names, codes, and aliases from its own table. |

Decisions:

- The folder spelling to give agencies is the language name in lower case with an underscore
  between words: `spanish`, `portuguese`, `haitian_creole`. It works in every tool above.
  `haitian-creole` and the code `ht` are also accepted by the school, crisis message, teaching
  sheet, and local-fill tools. `docs/tools.md` says this, and says where a tool is stricter.
- The shelter intake tool now uses the shared `language_folder` rule (in this review it was
  changed from the raw cell text, which could name a folder with a space or point outside
  `/data/templates`). Like the LTBI calendar and newcomer checklist, it matches a code only to a
  folder named with that code.
- New tools that read `/data/templates/` use `TemplateResolver`. Moving the other lookups onto
  it, so that codes and names match everywhere, is follow-up work and is not done in this
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

### 5. New decision: staff contacts are filled in only on an exact full-name match

Decided by the product owner on 2026-11-11. ADR 0009 has the Phase 13 tools fill staff phone
numbers and emails from a roster file. Before this review, a name that was given but not found
on the roster fell back to the first person holding the same position, so another person's
contact details could appear next to the name.

- When a name is given (an ICS position holder, a successor, an exercise point of contact, a
  corrective-action owner), contact details are filled in only when the roster has that exact
  full name, ignoring case and punctuation. Initials or partial names do not match.
- A name that is not on the roster gets no contact details; the form or plan shows the name
  alone and the result counts it as unmatched.
- The position (its first holder) is used only when no name is given.

## Consequences

- An agency that keeps one folder per language should name two-word languages with an
  underscore. Until the lookups are unified, a file that records languages as codes ("es",
  "ht") needs folders named with those codes for the LTBI calendar, newcomer checklist, and
  shelter intake forms.
- Rosters must spell names the way requests do. A nurse who sees a staff member's contacts
  missing from a form should check the spelling of the name in the roster and the request.
- Run outside the container (for example `cargo run` during development), the server uses the
  computer's own time zone.
- Dates the nurse gives explicitly are not affected by the time zone; only defaults and printed
  timestamps are.
