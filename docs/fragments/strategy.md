# Fragment: Phase 18 strategy (LEAD-03, LEAD-04, LEAD-05)

The coordinator folds these sections into `CHANGELOG.md`, `docs/acceptance/phase-18.md`, and
`STATUS.md`. Package key `strategy`; branch `phase-18-strategy`.

## Changelog

- Leadership tools (toolset `leadership`):
  - `assess_foundational_public_health_services` (LEAD-03): a gap analysis against the
    Massachusetts local public health standards (G.L. c. 111, s. 27D, the State Action for Public
    Health Excellence law enacted in 2020) and the Foundational Public Health Services
    framework. Four sets are rated by the nurse (the tool never fills in an answer): DPH's
    Performance Standards (93 mandated local responsibilities from DPH's Excel workbook), DPH's
    workforce standards (6 positions and 7 inspection types), the 56 FPHS Headline
    Responsibilities, and the 4 program terms in s. 27D(a) with no single FPHS counterpart.
    Without statuses it writes a self-assessment worksheet (XLSX) and guide (DOCX); with typed
    statuses or the filled worksheet it returns counts by set and area, a gap table, a three-tier
    priority view (legal mandates first; the nurse can override any item), and shared-service
    options (statute models, Public Health Excellence grants, training hubs, and the shared service
    arrangement DPH's April 2026 list gives for the town), and writes the gap analysis (DOCX and
    XLSX). Two new snapshots: `ma-performance-standards` and `ma-shared-service-arrangements` (351
    municipalities), built by `xtask/scripts/build_strategy_snapshots.py` (with `--check`).
  - `phab_accreditation_crosswalk` (LEAD-04): maps the nurse's own policies, plans, and reports to
    the 10 domains and 20 standards of PHAB Standards & Measures Version 2022 with the Phase 1
    document index: strong, some, or no evidence per standard, the documents (the nurse's file
    names), the cues matched, passages of at most 20 words with names and contact details redacted,
    gaps, and an optional measure-level crosswalk from a measures list in the nurse's own PHAB
    copy. Files that look like client records or interview transcripts are left out. DOCX and XLSX
    crosswalk in `/data/output`. PHAB's measure text is not bundled (copyright; the PHAB site was
    unreachable).
  - `build_strategic_plan` (LEAD-05): scaffolds a strategic plan from content the nurse supplies:
    SWOT or SOAR prompts beside the nurse's entries, a stakeholder power and interest grid
    (accessible SVG and PNG with a text key, plus a table), coded priorities, goals, objectives, and
    strategies, the link to the community health improvement plan (CHIP), a blank-field check, and a
    draft DOCX. PHAB's strategic plan requirements are reported as unverified.

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Make me a self-assessment worksheet for our local public health standards. We are in Amherst." | `assess_foundational_public_health_services` | `mode` is worksheet; 166 standards in four sets; nothing is rated; XLSX (first sheet "Self-assessment") and DOCX in `/data/output`; shows the Hampshire Public Health Shared Services Collaborative and its training hub for Amherst; the caveats say the law is the 2020 SAPHE law (G.L. c. 111, s. 27D), that "SAPHE 2.0" was not found, and that the content is pending review. |
| 2 | "Here is my filled worksheet (`fphs_assessment.csv`). What are our gaps and what should we do first?" | `assess_foundational_public_health_services` (`assessment_path`) | Gap counts by set and area; the highest-tier gaps listed first (a mandated responsibility not met is tier 1); the tier rules are shown; the owners' names, phones, and emails and the notes in the file do not appear in the response; XLSX/DOCX written with the notes. |
| 3 | "Mark PS-DIS-003 not met and FPHS-07.01 partly met, delivered by a shared arrangement; high priority for the second." | `assess_foundational_public_health_services` (`statuses`) | Two gaps; the second moves to tier 1 with "your priority" as the source; delivered-by counts show the shared arrangement. |
| 4 | "Use line_list.csv as the assessment file." | `assess_foundational_public_health_services` | Refused: not the self-assessment worksheet; no names, dates, or other values from the file appear. |
| 5 | "Which PHAB standards are covered by the documents in /policies?" (put a community health assessment, a CHIP, and a strategic plan in the folder first) | `phab_accreditation_crosswalk` | 20 standards in 10 domains; standards 1.1, 5.2, and 10.1 show some or strong evidence with the file names; standards with nothing are listed as gaps; passages are at most 20 words; the caveats say a keyword match is not a finding that a measure is met and that PHAB's measure text could not be read (copyright). |
| 6 | "Run the PHAB crosswalk on the whole data folder." | `phab_accreditation_crosswalk` (`folder` ".") | Client notes, 51A notes, the client letter, and the focus group and interview transcripts are left out and counted; none of their file names, names, phones, or addresses appear in the response; the output workbook's "Left out" sheet lists them. |
| 7 | "Use my PHAB measures list (`phab_measures.xlsx`: Measure ID, Short title, Keywords) and show which measures have evidence." | `phab_accreditation_crosswalk` (`measures_path`) | Measure IDs with evidence levels and the standard each belongs to; the short titles appear only in the output files; a roster given as the measures file is refused. |
| 8 | "Start a 2027 to 2031 strategic plan for our board of health. Strengths: trusted nurses. Weaknesses: no epidemiologist. Stakeholders: board of health (power 5, interest 5), select board (5, 2), parent groups (2, 4)." | `build_strategic_plan` | SWOT table with the entries beside prompts and the four strategy-pairing questions; grid quadrants Manage closely, Keep satisfied, Keep informed; SVG and PNG have alt text and a key; DOCX in `/data/output` with "[to be completed]" where nothing was given; the result says PHAB requirements are unverified. |
| 9 | "We have three priorities and a CHIP with four priorities. Which CHIP priorities have nothing behind them?" | `build_strategic_plan` | The CHIP table lists each CHIP priority with the strategic priorities linked to it; unlinked ones are findings; a priority that names a CHIP priority not on the list is reported; objectives without a measure, target, or date are flagged. |
| 10 | "Draft the plan using SOAR instead." | `build_strategic_plan` (`method` soar) | Strengths, opportunities, aspirations, and results prompts; no SWOT strategy-pairing prompts; the caveat says the SOAR source was a journal abstract. |

## Content review

- `content/leadership/fphs_framework.yaml` (`leadership-fphs-framework`, pending, due 2027-02-01).
  Check: (1) the **law naming**: the request and the plan said "2020 SAPHE 2.0 law" and "2024 SAPHE
  2.0"; mass.gov and the statute say the SAPHE Act came in 2020 (the grant program was renamed in
  April 2020), it is codified at G.L. c. 111, s. 27D, and the grants are now the Public Health
  Excellence (PHE) program; no page used "SAPHE 2.0". Confirm with OLRH and decide what the tool
  should call it. (2) The 56 FPHS Headline Responsibilities were copied from DPH's copy of the
  operational definitions (Word file modified 2024-11-05): high confidence they match that file;
  the **licence of the PHAB/de Beaumont framework was not read**. (3) The mapping from the
  statute's capability and program terms to FPHS areas, and the choice of four "own items" (public
  health nursing services, epidemiology, food and water protection, data-driven interventions),
  are this project's reading: medium confidence. (4) The four-value status scale, the three
  priority tiers, and the shared-service decision questions are this project's drafts (low to
  medium). (5) The PHE grant figures (FY2027 up to $26.6 million, 54 arrangements, 322
  municipalities) are as the program page read on 2026-11-12.
- `snapshots/ma_performance_standards.json` (`ma-performance-standards`; builder
  `xtask/scripts/build_strategy_snapshots.py`). 93 mandates from the four mandate sheets (Admin 8,
  Disease control 21, Environmental health 57, Tobacco 7), 6 positions and 7 inspection types from the
  Workforce Standards sheet. IDs (PS-ENV-012) are this project's, in sheet order: they change if DPH
  inserts rows, so an old filled worksheet may map to different standards after a refresh (the
  worksheet has the standard's text beside the ID; consider matching on citation). The "Frequently
  Used", "Resources", and "Keywords" sheets are not read. Summaries are DPH's one-line text; the
  citation controls. Seven standards carry DPH's * (possibly outdated). Phone numbers and emails
  were dropped.
- `snapshots/ma_shared_service_arrangements.csv` (`ma-shared-service-arrangements`): 351
  municipalities, April 2026 list; 29 have no arrangement ("N/A"; includes the standalone
  grantees Boston, Springfield, Worcester); two are labeled "PHE: Capacity Building Grantee".
- `content/leadership/phab_standards.yaml` (`leadership-phab-standards`, pending). **PHAB's own
  document could not be fetched** (phaboard.org refused; so did every mirror tried). The 10 domain
  names and 20 standard numbers and titles come from CDC's 2023 HAI/AR crosswalk toolkit (medium
  confidence). **No measure IDs or measure titles are bundled** (the brief asked for IDs; none
  could be verified). The keyword cues, document types, and match rules are this project's (low
  confidence). The month PHAB adopted Version 2022 (February 2022) is from a search summary
  (low). Decide whether the nurse wants to supply a measures list (the tool supports it).
  PHAB's licence terms were not read.
- `content/leadership/strategic_planning.yaml` (`leadership-strategic-planning`, pending).
  **PHAB's strategic plan requirements are unverified** (what was read: CDC's description of
  Standards 10.1 and 5.2, CDC's statement that a CHA and CHIP dated within five years are required,
  and a 2025 open-access study of plan contents). The SWOT, SOAR, and stakeholder prompts, the
  four quadrant names, and the plan outline are this project's drafts (SOAR: one journal abstract;
  stakeholder analysis: one review abstract; SWOT: no source read, low).

## Open items

- **PHAB measures:** PHAB's Version 2022 document needs to be read by a person (phaboard.org is
  blocked from the build network); then measure IDs, short titles, and the documentation each
  requires can be added (their copyright and PHAB's terms apply) and the strategic plan
  requirements verified. The crosswalk's evidence is keyword-based and does not read document
  dates, approval, or currency; PDFs without text (no OCR) are skipped.
- **Crosswalk privacy rules** are heuristics: a file is left out when it shows a labeled birth date,
  record or health plan number, two or more street addresses, a "Client name:" style label, or five
  or more "Name:" speaker lines; passages are not shown from files with phone numbers, email
  addresses, or street addresses, and are strictly redacted otherwise. A file that does not trip
  these but holds personal details in prose could still contribute a short passage.
- **Worksheet IDs** (PS-xxx-nnn) are positional in DPH's workbook; see the snapshot note above. DPH
  says the next Performance Standards will include the FPHS framework: re-read the workbook then and
  re-map the sets.
- **Decisions for the nurse:** the name of the law ("SAPHE" 2020 vs. "SAPHE 2.0" 2024); whether the
  priority tiers should weight workforce standards as mandates (the statute names workforce standards
  in s. 27D(c)(ii)); whether to add DPH's discretionary board of health powers ("Frequently Used"
  sheet) as a fifth set; whether city-only or town-only mandates need a "not applicable" default.
- **XLSX worksheet** has no drop-down lists for Status (the shared writer makes plain tables); the
  accepted values are in the guide and the error message.
- **No `--check` of the FPHS Headline Responsibilities:** they were copied once from the Word file;
  there is no builder for them yet (the file's SHA-256 is not recorded in the YAML).
- **Network lessons:** mass.gov works with a plain `curl/8` user agent (Excel and Word files under
  `/doc/<slug>/download` return the files; last-modified headers give the file dates);
  malegislature.gov works; cdc.gov PDFs work; phaboard.org, health.mn.gov, slocounty.ca.gov,
  archive.phf.org, letsgethealthy.ca.gov, and clearimpact.com are blocked (WebFetch could not resolve
  phaboard.org either). PubMed E-utilities (esearch, esummary, efetch for PubMed and PMC) work.
- Cargo contention: eight helpers building at once on four cores made a single `cargo build`
  take many minutes; the shared lock on `target/` stalled a second cargo in the same worktree.
