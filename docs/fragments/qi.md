# Fragment: Phase 16 `qi` (EBP-05)

## Changelog

- `plan_quality_improvement` (EBP-05, toolset `ebp`): guided quality improvement planning with
  the Model for Improvement. From what the nurse supplies it builds an aim statement (how much,
  for whom, by when) with checks; a family of outcome, process, and balancing measures with
  operational-definition gaps; a driver diagram, a fishbone diagram, and a process map, each as
  Mermaid text, an accessible SVG and PNG (role, title, description; shapes and labels, not color
  alone), and a text-alternative outline (blank scaffolds when nothing is given); PDSA cycle plans
  with checks taken from a systematic review of PDSA reports; and the SQUIRE 2.0 checklist (item
  numbers and names with the tool's own paraphrases and a license note; SQUIRE is copyrighted). From
  the nurse's measure data (values typed in, a file with one row per event grouped by period, or a
  one-row-per-period file such as the tracker) it draws a run chart and applies the IHI/Perla rules
  (shift of 6, trend of 5, too few or too many runs, an astronomical-point screen) and the Anhøj rules
  (long run, crossings), or an XmR, p, u, or c control chart through the existing `phn-epi` SPC
  engine; a new phase gets its own median or limits. Only period aggregates are returned, small
  counts are hidden, and the median, limits, and phase averages are withheld when any period is
  hidden. It writes a DOCX plan (diagrams as images with alt text and as outlines), an XLSX tracker
  with live formulas (run length, shift, trend, runs, Anhøj limits, XmR or p limits, drop-downs, a
  native chart) that were recalculated in LibreOffice and matched an independent computation, and
  the diagrams and chart to `/data/output`. Content `content/ebp/quality_improvement.yaml`
  (`status: pending`, `review_due: 2027-02-01`). Golden tests:
  `crates/phn-tools/tests/qi_golden.rs` (run chart rules against an independent Python computation,
  `tests/fixtures/generators/qi_run_chart_golden.py`; the Anhøj prediction limits against Table 1 of
  the published paper for 10 to 100 useful points; XmR, p, u, and c limits).

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Help me start a quality improvement project to raise on-time vaccination in our clinic. I do not have a plan yet." | `plan_quality_improvement` (no arguments) | Three Model for Improvement questions; an aim template and a list of what is missing (population, numbers, time); blank driver diagram, fishbone, and process map (each as Mermaid text, with an outline and files in `output/`); the SQUIRE checklist with a copyright note; a Word plan and Excel tracker in `output/`. The first caveat says the content is pending review. |
| 2 | "Our aim is to raise the share of two-year-olds up to date from 68% to 80% at the Elm Street clinic by June 30, 2027. Write the aim statement and tell me if it is complete." | `plan_quality_improvement` (`aim`) | "We will increase ... from 68% to 80% by June 30, 2027." Checks for population, numbers, time, and direction are met; "ambitious yet feasible" is left as a team judgment. Remove the date and a check says to give one. |
| 3 | "Here are my measures: percent up to date (outcome), percent with a reminder sent (process). Is my family of measures complete?" | `plan_quality_improvement` (`measures`) | A missing balancing measure is flagged with prompts (what could get worse, who would notice first); the reminder measure lists the numerator, data source, frequency, and owner it still lacks. |
| 4 | "Build a driver diagram, a fishbone, and a process map for late immunization records from these notes." (Give drivers, causes under people and methods, and five process steps with a decision whose "no" loops back.) | `plan_quality_improvement` | Mermaid for each diagram; open the SVG and PNG and check the layout, that the "no" arrow returns to the right step, that pain points have an outline and an exclamation mark (not color alone), and that screen-reader text (title and description) is present. The outline repeats everything in the picture. |
| 5 | "Plan three PDSA cycles: call 5 families, then 20, then mail letters." | `plan_quality_improvement` (`pdsa_cycles`) | Checks: a prediction is required before each test, small scale stated, data over time, later cycles say what they build on (flagged if not), one change per cycle (flagged if a description joins two changes). |
| 6 | "Here are 20 monthly counts of reminder calls per week, with a change in month 13. Did anything shift?" (Type 12 values near 5 then 8 values near 10.) | `plan_quality_improvement` (`values`, `labels`) | IHI shift (M13 to M20) and Anhøj long run reported, with the sentence that this is the direction the aim wants when an aim direction is given; the note that the IHI rules gave more false signals in published testing; run chart PNG and SVG with a text alternative; the Mermaid line chart. |
| 7 | "Use qi_clinic_measure.csv: Month is the period, Children Up To Date over Children Seen. We started reminder letters in 2025-10." | `plan_quality_improvement` (`path`, `period_column`, `numerator_column`, `denominator_column`, `phase_starts`) | 20 periods; 2026-04 (3 of 6) is hidden and the median, limits, and phase averages are not shown; the chat has no names, phone numbers, or emails from the file although it has a contact person, cell, and email; the chart and tracker in `output/` have every value. |
| 8 | "Use that file again, but average Contact Cell." | `plan_quality_improvement` (`numerator_column` "Contact Cell") | A tool error that says the column looks like phone numbers and offers de-identification; no value is repeated. |
| 9 | "Open the tracker. Add a month and a value." | (Excel) | On the Data sheet the new row's median, run length, shift, trend, and limits fill in; the Summary sheet updates its signals; Settings holds the baseline and rule lengths in yellow cells; the PDSA and SQUIRE sheets have drop-downs. Check the chart range (extend it by hand). |
| 10 | "Where are we on the SQUIRE checklist? Items 1 to 6 are drafted and 7 is not applicable." | `plan_quality_improvement` (`squire_status`) | Counts (6 done, 1 not applicable, 11 to do); each item shows the plan section it draws on; the copyright note and the link to the guideline; the "cite SQUIRE when you use it" instruction. |
| 11 | "Is this project quality improvement or research?" | (none) | The tool does not decide: its caveat says this is for the agency or IRB, and its next steps repeat that. (A separate tool is planned in Phase 18.) |

## Content review

`content/ebp/quality_improvement.yaml` (pending; every item has a source, section, and
confidence). The nurse should check:

- **Model for Improvement and aim wording** (high): the three questions, the aim attributes
  (specific, numeric, time-bound; "how much improvement, for whom, by when"), and the family of
  measures are from a 2026 open-access guide (CC BY) that quotes them. The wording is not from IHI:
  its pages were blocked. Check it against IHI's current Model for Improvement and PDSA worksheets.
- **Measure type definitions** (medium): the one-line meanings of outcome, process, and balancing
  measures are the tool's plain-language summaries of what two open-access articles say; check
  them against the definitions you teach. The six operational-definition fields are the tool's
  checklist, not a published standard.
- **Driver diagram, fishbone, and process map** (high for the structure, medium for the process
  map): the structure comes from the core components guide and a fishbone primer; the five default
  fishbone categories are the primer's (people, methods, materials, equipment, environment). The
  process-map tips are drafting guidance by this tool (low).
- **PDSA** (high): the Plan, Do, Study, Act questions and the seven application checks come from a
  systematic review of PDSA reports. "Adopt, adapt, or abandon" (the Act step) is general
  knowledge of the IHI approach and is marked low confidence; nothing fetched states it.
- **Run chart rules** (medium to low): shift of 6 and trend of 5 (with a repeated value counted
  once) are Perla's rules as described in two Anhøj papers, because the Perla paper's full text
  was blocked (only its abstract was read): medium. The Anhøj limits are verified against the
  published table (high). The too-few and too-many runs limits are computed from the exact
  distribution of the number of runs at 2.5% per tail, and Perla's table was not compared (medium).
  **The astronomical-point rule is low confidence**: Perla's definition was not read; the tool's
  screen (modified z-score above 3.5 against the other points) is its own stand-in. Decide whether
  to keep it, change the threshold, or drop it.
- **SQUIRE 2.0** (high for the 18 item numbers and names, read in full from the open-access
  article; the descriptions are the tool's paraphrases). SQUIRE is copyrighted; the article's PMC
  license is CC BY-NC 4.0, but the SQUIRE website's own permissions could not be read. Check that
  the paraphrases and the license note are acceptable before the checklist goes in handouts.

Not bundled or verified: IHI pages (Model for Improvement, PDSA worksheet, run chart and driver
diagram tools), *The Improvement Guide*, the SQUIRE website and its explanation-and-elaboration
document, the full text of the Perla 2011 run chart paper, and any QI guidance specific to
Massachusetts DPH or the Massachusetts Quality Improvement collaboratives.

## Open items

- **Perla paper and IHI pages** could not be read (BMJ, IHI, and squire-statement.org were blocked
  from the build network; NCBI E-utilities and PubMed Central full text worked). If a reachable
  copy appears, compare the runs table and the astronomical-point wording and raise the
  confidence flags.
- **Astronomical-point screen** is the tool's operationalization (modified z-score above 3.5
  against the other points). It flags nothing in series shorter than 10 points and can flag several
  points in a bimodal series. Decide whether the nurse wants it.
- **Rule sets:** by default both IHI/Perla and Anhøj rules are shown, with a note that the Anhøj
  rules did better in published simulations and the Perla trend and too-many-runs tests were judged
  not useful. A nurse whose course or agency requires the IHI rules can pass `rule_set: ihi`.
- **Tracker limits:** the native chart covers the rows that had data when the workbook was made;
  periods added later need the chart range extended by hand. Formulas assume no blank rows in the
  middle of the Data sheet. The runs-test table on the Settings sheet splits the points as evenly
  as possible above and below the median (the tool's response uses the actual numbers). Shift and
  trend flags in the sheet appear from the point where the run reaches 6 or 5; the tool's response
  marks every point in the run. The astronomical-point rule is not computed in the sheet. The
  formulas use `CRITBINOM` (Anhøj crossings limit), which every Excel and LibreOffice version
  has; formulas were checked in LibreOffice only, not in Excel.
- **Checking the tracker:** `crates/phn-tools/tests/fixtures/generators/check_qi_tracker.py` is an
  opt-in script (not part of CI; needs LibreOffice and openpyxl). The ignored unit test
  `golden_workbooks::write_golden_trackers` writes the workbooks it recalculates.
- **Small-cell rule for a period file without a denominator:** counts from 1 to threshold - 1 are
  hidden unless `measure_kind` is `mean` (the numbers are then treated as averages). A file of
  other kinds of values (rates) should be given as `mean`.
- **Diagrams:** layouts are computed from text width (DejaVu Sans) with no overlap handling beyond
  wrapping; very long labels make large images. The fishbone SVG places causes along the bones in
  order and has no sub-causes. The process map draws one "no" branch per decision.
- **Shared-file edits outside the markers:** `crates/phn-tools/src/ebp/spc.rs` (EBP-06) now exposes
  `Prepared` and `prepare_file` as `pub(crate)` so the QI tool reuses its event-file grouping and
  suppression; no behavior change. The `content/data_sources.yaml` registry has no Phase 16
  marker, so no entry was added for `ebp-quality-improvement`: add one when merging (kind
  `content`, `used_by: [plan_quality_improvement]`, pending-review caveat).
- **Fixtures:** `fixtures/synthetic/data/qi_clinic_measure.csv` (20 months, contact person, cell,
  and email columns; generator `fixtures/synthetic/generators/phase16_qi_fixture.py`, entries added
  to `seeded_identifiers.json`). The fixtures README table was not touched; add a row.
- **Network lessons for STATUS.md:** NCBI E-utilities `efetch db=pmc` returns open-access full
  text as XML (SQUIRE 2.0, PLoS One run chart papers, a PDSA review, QI methods articles) when
  ihi.org, squire-statement.org, pmc.ncbi.nlm.nih.gov, bmjqs.bmj.com, journals.plos.org, doi.org,
  europepmc, api.crossref.org, ahrq.gov, hrsa.gov, and health.gov return 403 at the proxy.
  `esearch db=pmc` returns PMC identifiers as bare numbers (prefix `PMC`).
- **Phase 18 (EBP-07):** the QI-versus-research determination is not built; the tool only says
  the question exists.
