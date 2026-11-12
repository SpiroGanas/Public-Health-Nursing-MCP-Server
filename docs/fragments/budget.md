# Fragment: Phase 17 `budget` (PLAN-09, PLAN-10)

## Changelog

- `build_program_budget` (PLAN-09, toolset `planning`): a line-item program budget in federal object
  classes (personnel, fringe benefits, travel, equipment, supplies, contractual, other, and indirect
  costs, labeled in the order of SF-424A Section B) with up to five years, yearly escalation, cost
  share and a required-match check, and indirect costs on the right base. Indirect options are the
  de minimis rate (up to 15 percent of modified total direct costs, 2 CFR 200.414(f) as the eCFR
  publishes it, current through 2026-11-09; a higher rate or another base is refused) or a negotiated
  rate on MTDC, total direct costs, salaries, or salaries plus fringe. MTDC follows 2 CFR 200.1:
  equipment, participant support, rental, patient care, tuition and scholarships, and the part of a
  subaward above $50,000 (counted over the whole award, not per year) are left out. Lines are typed
  into the request or read from a CSV/XLSX in `/data` through the local-data guardrails (a file with
  a name, phone, email, or other identifier column is refused and no cell is echoed). It writes an
  XLSX workbook with live formulas (Inputs, Line items with spare rows, Budget summary with SUMIFS by
  object class, CHOOSE on the indirect base, a match check, and an arithmetic check) and a DOCX
  budget justification drafted from the line items with `[NURSE: ...]` placeholders. The response
  returns category totals by year, rates, structure, and the cited rules, never individual salaries or
  lines. The formulas were recalculated in LibreOffice and matched an independent Python computation
  with exact decimals (341 cells, 0 differences). Content `content/planning/budget_reference.yaml`
  (`status: pending`, `review_due: 2027-02-01`). Golden tests: `crates/phn-tools/tests/budget_golden.rs`
  with `tests/fixtures/generators/budget_golden.py` and `check_budget_workbook.py`.
- `economic_evaluation_calculator` (PLAN-10, toolset `planning`): cost per participant, average cost per
  outcome, the ICER against a comparator (dominant and dominated programs get no ratio), net benefit,
  benefit-cost ratio, return on investment, payback year, and break-even (value per outcome and share
  of expected outcomes), over up to 30 years with discounting (default 3 percent; year 1 undiscounted
  unless asked), a one-way sensitivity analysis (tornado table sorted by swing, flags where the
  conclusion changes, default 20 percent ranges unless given), and an accessible tornado chart (SVG and
  PNG, title, description, data table, and a text alternative) in `/data/output/charts`. The plain-language
  reading is guarded: it says the numbers are inputs and do not show causation, that an average cost per
  outcome is not incremental, that a benefit-cost ratio below 1 leaves out benefits not valued in
  dollars, and that no cost-effectiveness threshold is applied unless the nurse gives one. Content
  `content/planning/economic_evaluation.yaml` (`status: pending`, `review_due: 2027-02-01`). Golden
  tests: `crates/phn-tools/tests/economic_golden.rs` reproduces the benefit-cost results in Table 2 of
  CDC's MMWR 2024;73(31) (societal ratio 10.9, payer 3.3, net savings $2,663 and $540 billion) and
  compares the rest with an exact-fraction Python computation
  (`tests/fixtures/generators/economic_golden.py`).

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Build a one-year budget: a half-time public health nurse at $80,000, a $12,000 vaccine refrigerator, $3,000 of supplies, 25 percent fringe, de minimis indirect." | `build_program_budget` | Federal request $72,950: personnel $40,000, fringe $10,000, direct $65,000, indirect $7,950 on an MTDC base of $53,000 (the refrigerator is left out). Caveats say the de minimis rate is up to 15 percent (10 percent before October 2024) and that the funder's notice controls. A workbook and a justification are in `/data/output`; the justification has `[NURSE: ...]` placeholders. Open the workbook and change the fringe rate on Inputs: the summary recalculates. |
| 2 | "Build a 3-year budget from program_budget_lines.csv with 24.5 percent fringe, 2 percent raises, and a negotiated rate of 28.5 percent of MTDC. Check it against a 20 percent match." | `build_program_budget` (`line_items_path`) | Totals by object class and year only; no salary appears in the chat. The $30,000-a-year subaward counts $30,000 in year 1, $20,000 in year 2, and nothing in year 3 in the indirect base (the Line items sheet shows the MTDC base by year); participant incentives and the monitors are left out of the base. The match check shows the shortfall or "requirement met" against the cost share line. |
| 3 | "Use staff_roster.csv as the budget file." (a file with names and contacts) | `build_program_budget` | Refused with a privacy message naming the kind of column; no names, phones, or emails appear. |
| 4 | "Use a 20 percent de minimis rate." | `build_program_budget` | Tool error: the de minimis rate is at most 15 percent; suggests a negotiated rate if the organization has one. |
| 5 | "Start me a blank budget workbook for a 2-year project." | `build_program_budget` | A workbook with spare rows and live formulas; no justification; says no line items were given. |
| 6 | "Our program costs $100,000 a year for 3 years, serves 600 people, and averts 40 cases a year worth $3,500 each. Give me the cost per case, the benefit-cost ratio, and break-even." | `economic_evaluation_calculator` | Present-value cost $291,347 (3 percent, year 1 undiscounted); cost per outcome $2,500; benefit-cost ratio 1.40; return on investment 40 percent; break-even value per outcome $2,500; cost per participant $500. A caveat names the default discount rate and where it comes from; the reading says the numbers do not show causation. |
| 7 | "Which of those assumptions matters most? Show me a tornado chart, and test the value of a case from $2,000 to $4,200." | `economic_evaluation_calculator` (`sensitivity`) | A tornado table sorted by swing; the value of a case is marked as changing the conclusion (ratio 0.8 at $2,000); an SVG and PNG chart in `output/charts` with a title, a description, and a text alternative in the response. Open the chart and check it reads well. |
| 8 | "Compare a $120,000 outreach program that prevents 30 cases with a $40,000 usual-care program that prevents 10." | `economic_evaluation_calculator` (`comparator`) | ICER $4,000 per additional case; "more costly, more effective"; no threshold applied; says the tool does not decide what counts as cost-effective. |
| 9 | "Our new program costs $60,000 and prevents 35 cases; the old one costs $90,000 and prevents 20." | `economic_evaluation_calculator` | Classified as dominant with no ratio and the reason (a negative ratio has no meaning); reports the savings and extra outcomes. |

## Content review

- `content/planning/budget_reference.yaml`: the nurse must check (1) the quoted rules against 2 CFR
  200.1, 200.306, 200.414, 200.431, and 200.456 (read from the eCFR versioner API on 2026-11-12;
  high confidence), especially the de minimis rate (up to 15 percent of MTDC), the $50,000 subaward
  limit, and the $10,000 equipment threshold, which are the October 1, 2024 values; (2) the SF-424A
  object class lines 6a to 6k: **low confidence**, because grants.gov, the SF-424A instructions, and
  every mirror were blocked, and the list comes from a web-search summary of copies of the form
  (construction, 6g, is listed as not built); (3) the funder-check reminders that rest on "not read"
  (salary limits, page limits, escalation expectations: low); (4) the justification templates (the
  project's own draft wording, plain language, with `[NURSE: ...]` prompts).
- `content/planning/economic_evaluation.yaml`: the nurse must check (1) the 3 percent default discount
  rate: **medium confidence**, taken from how CDC's MMWR 2024;73(31) applied the Second Panel; the
  Panel's report (JAMA 2016;316(10):1093-1103) was read as an abstract only, and OMB Circular A-94 was
  not readable (whitehouse.gov blocked), so no federal discount rate is used; (2) the definitions of
  cost per outcome, ICER, return on investment, and break-even: standard textbook formulas that no
  fetched page spells out (medium); (3) the default sensitivity ranges (20 percent either way; discount
  rate 0 to 7 percent): the tool's own choices (low); (4) the interpretation rules that cite the CDC
  commentary on the Second Panel (Am J Prev Med 2018;54(4):600-602) for dominant strategies, outcome
  choice, and perspective (high for what it says).

## Open items

- **Not built:** no salary cap, no mileage or per-diem rates, no regional cost rates, no construction
  line (SF-424A 6g), no per-diem or participant-support rule beyond the MTDC exclusion, and no inflation
  index: every figure is the nurse's. No XLSX output for the economic calculator (the chart and the
  response carry the results). Budgets are limited to five years and 150 typed lines.
- **Decisions for the nurse:** whether to default yearly escalation to a non-zero rate (it is 0 so that no
  rate is invented); whether the de minimis default should ask for confirmation that the organization has
  no negotiated rate (the justification has a `[NURSE: ...]` prompt for it); whether the response may
  return a personnel total when there is one position (it does, with a caveat).
- **Privacy limits:** salaries typed into the request pass through the AI assistant; the tool says so and
  steers to `line_items_path`. The response never lists individual lines, but a category total for a
  one-person category equals that person's pay (caveat added). The workbook and the justification hold
  salaries and are written to `/data/output` only. A file is refused when any column is flagged as an
  identifier (the PHI detector is heuristic).
- **Technical debt:** the workbook holds at most five year columns (rebuild to change the number of years);
  the Inputs "Budget years" cell is informational. The workbook was checked in LibreOffice only, not in
  Excel. XLSX `ROUND` matches the Rust `round2` (half away from zero with a tolerance for binary noise on
  ties). `generators/check_budget_workbook.py` is a manual check, not part of CI (it needs LibreOffice).
- **Network lessons:** www.ecfr.gov web pages are blocked but the versioner API
  (`/api/versioner/v1/full/<date>/title-2.xml?part=200&section=<n>`) works if the request sends
  `Accept-Encoding: gzip` (curl `--compressed`); without it the API returns 406. Blocked in this
  environment: grants.gov (including apply07 and files.simpler.grants.gov), whitehouse.gov, hhs.gov,
  federalregister.gov, govinfo.gov, ncbi.nlm.nih.gov web pages and PMC, jamanetwork.com, doi.org,
  europepmc.org, ed.gov, usda.gov, doi.gov, and most other agency sites. Reachable: cdc.gov (MMWR pages
  and Stacks PDFs by id), epa.gov (but not www3.epa.gov), census.gov, cms.gov, mass.gov, and NCBI
  E-utilities (PubMed records).
