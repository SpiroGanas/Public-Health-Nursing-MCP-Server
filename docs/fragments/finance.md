## Changelog

- Leadership and finance tools (toolset `leadership`):
  - `analyze_health_department_budget` (LEAD-01): analyzes the department's budget or ledger file
    (CSV or XLSX in `/data`, one row per account line or one row per line per month) and returns
    category totals only: year-to-date spending and revenue against the share of the fiscal year
    passed (July 1 start by default, configurable), money available after encumbrances, flags for
    lines over budget or ahead of or behind pace, the revenue mix (fees and revolving funds,
    municipal appropriation, state Public Health Excellence grants, other state, federal, other
    grants), cost per service from volumes and allocation shares the nurse supplies, budget per
    resident beside DPH's published 2022-2023 figures, and year-end forecasts by two labelled
    methods (straight line, and recent run rate for monthly ledgers). Vendor, employee, and payee
    columns are never read; all salary lines are combined into one row that is hidden everywhere
    (response, workbook, report) when fewer than three positions stand behind it. Writes an XLSX
    with live formulas and a DOCX summary to `/data/output`. Content `content/leadership/
    department_finance.yaml` (pending review).
  - `model_staffing_needs` (LEAD-02): estimates public health nurse FTE from annual volumes and
    minutes per unit the nurse supplies (typed in, or from a CSV/XLSX read through the local-data
    guardrails), with an adjustable productive-hours calculation (the defaults are labelled
    placeholders), scenarios (growth, added services), the gap to current FTE, and residents and
    clients per nurse. No staffing benchmark is bundled (none could be read from a fetchable
    source); it compares only with a benchmark the nurse gives. Writes an XLSX with live formulas
    and a DOCX summary. Content `content/leadership/staffing_model.yaml` (pending review).
- Synthetic fixtures `health_dept_budget.csv`, `health_dept_budget_small.csv`,
  `health_dept_ledger_monthly.csv`, and `staffing_volumes.csv` (seeded vendor, employee, and
  contact names, emails, and phone numbers), and the golden-value generator
  `crates/phn-server/tests/fixtures/phase17_finance/generate_expected.py`.

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Analyze health_dept_budget.csv as of October 31, 2026 and tell me where we are ahead of or behind pace. Use the Positions column." | `analyze_health_department_budget` | FY2027 (July 1, 2026 to June 30, 2027), 123 of 365 days (33.7%) elapsed; expenses total 392,500 budgeted and 139,100 spent (35.4%, +1.7 points vs pace) with 14,300 encumbered; Software and Licenses flagged Over budget (7,600 spent of 7,500); Contracted Services shows its encumbrances; personnel is one combined row. Totals match `expected_finance.json`. |
| 2 | "What share of our revenue is PHE grant money, and what is municipal appropriation?" | `analyze_health_department_budget` | Revenue mix by group: the State PHE group (150,000 budgeted, 75,000 received) is about 36% of budgeted revenue and municipal appropriation about 38%; "Sale of publications" is listed as not classified and can be placed with `revenue_group_overrides`. No claim about what PHE may pay (award terms not read). |
| 3 | "Cost per dose for our 640 flu and other clinic doses: all the vaccine and medical supplies, a quarter of nursing time, and a 10 percent indirect rate; we planned 1,800." | `analyze_health_department_budget` (`services`, `indirect_rate_percent`) | Cost per unit 61.49 and budgeted cost per planned unit 60.50 (the nurse's shares and rate are stated as hers). |
| 4 | "Use health_dept_ledger_monthly.csv (month and account columns). Where will we end the year at the recent run rate versus straight line?" | `analyze_health_department_budget` | Layout "monthly ledger"; both forecasts shown for each category; the run-rate note names the three most recent complete months (2026-08 to 2026-10); each account's annual budget counted once; one-time spikes noted as treated as ongoing. |
| 5 | "Now analyze health_dept_budget_small.csv with the Positions column." | `analyze_health_department_budget` | Personnel row hidden (fewer than 3 positions); totals say they leave personnel out; revenue minus expenses withheld; a service that uses personnel is withheld; no salary figure in the chat, workbook, or Word summary. |
| 6 | "Analyze health_dept_budget.csv using the Vendor column as the category." (also Employee) | `analyze_health_department_budget` | Tool error saying vendor and employee columns are never read; no vendor, employee, email, or phone appears. |
| 7 | "Analyze line_list.csv as a budget." | `analyze_health_department_budget` | Tool error (no budget column found, or identifier columns refused); no values from the file appear. |
| 8 | "Open the workbook the budget tool wrote. Change the as-of date and the tolerance." | workbook | Share of year elapsed, variances, statuses, and forecasts recalculate (formulas, not typed numbers); the yellow cells are inputs. |
| 9 | "How many nurse FTE do we need? Home visits 1,200 a year at 45 minutes, clinic doses 3,000 at 6, TB case management 80 at 120, case investigations 150 at 60. We have 1.5 FTE, serve 24,000 people, 340 active clients." | `model_staffing_needs` | 1,510 hours; productive hours per FTE 1,427.2 with each leave and training input labelled "tool default (placeholder, no published source)"; 1.06 FTE needed, rounded up to 1.1 (0.4 fewer than now); 16,000 residents and 226.7 clients per FTE at current staffing; says no benchmark is bundled. |
| 10 | "What if volume grows 10 percent, or we add 120 diabetes prevention sessions of 90 minutes?" | `model_staffing_needs` (`scenarios`) | Scenario table with hours, FTE, rounded FTE, and gap; matches `expected_finance.json`. |
| 11 | "Use our leave policy: 1,950 paid hours, 80 holiday, 100 vacation, 56 sick and personal, 16 training, 15 percent non-service; round to half FTE." | `model_staffing_needs` | Every hours input marked "you gave this"; productive hours 1,443.3 per FTE; FTE rounded up to 0.5 (see `expected_finance.json`). |
| 12 | "Is there a standard of one public health nurse per 5,000 residents?" | `model_staffing_needs` / none | The tool does not confirm or bundle a ratio; it lists what was looked for and not used (the ratio, NACCHO, PHAB, SAPHE 2.0 and the capacity assessment, time standards). |
| 13 | "Compare with our agency target of one nurse per 8,000 residents (Board of Health policy 2025)." | `model_staffing_needs` (`benchmark`) | Comparison labelled as the nurse's own, with her source; implied FTE = population / 8,000; "did not verify it". |
| 14 | "Read staffing_volumes.csv for the volumes." | `model_staffing_needs` (`volumes_path`) | Same numbers as 9; the file's contact people never appear; privacy note says only three columns were read. |

## Content review

- `content/leadership/department_finance.yaml` (id `leadership-department-finance`, pending, review
  due 2027-02-01):
  - Read on 2026-11-12 and high confidence: DLS's Municipal Finance Glossary (fiscal year July 1 to
    June 30, encumbrance, appropriation, revolving fund; paraphrased), G.L. c. 44, section 56, DPH's
    Public Health Excellence page (FY2027 up to $26.6 million in state and federal funds, FY2026
    positions funded including 46 public health nurses, shared services arrangements and three
    standalone municipalities), DPH's archived SAPHE page (the 2020 naming history), the MAHB SAPHE
    2.0 guidance, and the capacity assessment's published budget per capita averages ($17.56
    municipal, $26.61 combined).
  - **Medium confidence:** the $36.98 per capita estimate (DPH cites "current research" without
    naming it).
  - **Not read / low confidence:** the PHE request for responses and award contract (allowable costs,
    payment and reporting terms are not stated by the tool); the MMA notice (host blocked). DPH's PHE
    page gives 54, 56, and 57 for grantee and arrangement counts, so none is bundled; check which is
    right.
  - **Tool defaults, not published standards (check them):** the revenue group word lists, the
    salary-word list, the 10-point pace tolerance, the minimum of 3 positions, and the 3-month run
    rate. Whether the PHE label words (PHE, SAPHE, public health excellence, shared services) match
    your ledger.
- `content/leadership/staffing_model.yaml` (id `leadership-staffing-model`, pending): **every
  productive-hours default except 2,080 (= 40 x 52) is a placeholder with no source** (holiday 88,
  vacation 120, sick and personal 64, training 24, non-service 20 percent): replace with agency and
  contract values. **No benchmark is bundled.** Not fetched: NACCHO's workforce staffing benchmarks,
  the PHAB workforce calculator, PubMed Central pages (hosts blocked), OPM and BLS (hosts blocked,
  would have sourced the 2,087-hour divisor and leave data). A 1 PHN per 5,000 ratio appeared only in
  a search result about a 2022 AJPH article whose text could not be read; it is listed as not used.

## Open items

- Decide whether to supply an agency-approved benchmark and time standards; if NACCHO or PHAB pages
  become reachable, read them and add cited benchmarks with a confidence level.
- Read the PHE request for responses and award contract and add what PHE may pay and how awards are
  reported (the tool currently says it did not read them).
- Personnel privacy rule (minimum 3 positions; salary words) is this project's; confirm it meets
  agency and union expectations. The count is the smaller of the positions column and the headcount
  given; with neither, the number of salary accounts is a proxy and can overstate the group.
- **Decision on `localdata`:** the PHI detector flags any header containing "account" or "acct" as
  account numbers, which refused every general ledger's Account and Account Category columns. The
  budget tool accepts a column whose only finding is that header word (`ledger_column` in
  `crates/phn-tools/src/leadership/budget.rs`), still scans its values for identifiers, and never
  returns account codes. Any other finding still refuses the column. Confirm this narrow exception
  is acceptable, or change the detector's header rule for ledgers.
- Straight-line pace ignores seasonality; the run rate treats a one-time spike as ongoing. A
  seasonal profile (for example fee revenue by month) would need the nurse's own history.
- Revenue groups come from label words; a chart of accounts mapping file would be more reliable.
- The tool does not read general-ledger exports with merged cells or multi-row headers (use a flat
  table with one header row); a monthly ledger needs one account column.
- No chart output for either tool (tables and workbooks only).
- Network: mass.gov pages and the capacity assessment (a .docx despite its URL) fetched with plain
  `curl`; www.opm.gov, www.bls.gov, www.mma.org, phaboard.org, essentialelements.naccho.org, and
  pmc.ncbi.nlm.nih.gov were blocked (E-utilities for PubMed and PMC abstracts work).
