# ADR 0004: Privacy guardrails for local data

- Status: accepted (Phase 1)

## Context

Everything a tool returns is sent to the AI vendor. Agencies generally have no business
associate agreement with that vendor, so identifiable client information must not reach the
model (tools that would need it are in Appendix A of the plan).

## Decision

1. **Sandbox:** paths are resolved inside `/data`; `..` is rejected and canonical paths must stay
   inside the data folder, so symbolic links cannot escape. Writes go to `/data/output` and never
   overwrite.
2. **Aggregate first:** tabular tools return structure and statistics by default. Row-level
   samples need `include_rows` and are always masked (identifiers hidden, dates to year, ZIP to
   three digits, ages over 89 as 90+). There is deliberately no option to unmask.
3. **PHI detection:** column names, value patterns, and public name lists, combined, erring
   toward flagging. In running text, names are found from titles, first-name + surname pairs at
   every position, relationship words ("Daughter Carmen"), and propagation of a detected surname
   to its other mentions.
4. **Redaction levels:** standard (SSNs and labeled DOB/record/member/account/license numbers)
   suits agency documents with staff contacts; any such finding escalates the document to strict
   redaction automatically.
5. **Suppression everywhere:** frequency tables pass through primary and complementary
   suppression at the configured threshold.
6. **Local mail-merge:** merged documents are written to disk; responses contain counts and
   generic file names only.
7. **Safe Harbor tool:** removes or generalizes the 18 identifiers, keeps no crosswalk, and warns
   about rare combinations of the remaining quasi-identifiers.

A synthetic corpus with 266 seeded identifiers and integration tests enforce these rules.

## Consequences

Some useful detail (exact dates, towns in row samples) is withheld from the model by default.
Automated detection can miss identifiers, so outputs still need the nurse's review; tool
responses say so.
