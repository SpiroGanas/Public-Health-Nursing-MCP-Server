## Changelog

- `build_logic_model` (PLAN-01): a program logic model from the nurse's description (inputs,
  activities, outputs, short-term, intermediate, and long-term outcomes, assumptions, and
  external factors), organized as CDC's Program Evaluation Framework (2024, Step 2) describes.
  Checks structure and wording with rules kept in `content/planning/logic_model.yaml` (missing
  activities or outcomes, outcomes written as activities, outcomes that do not say what changes,
  boxes over the one-page limit, missing assumptions, context, stage, or population), reads the
  model as CDC's if-then statements, drafts CDC's six-part narrative, and draws it as Mermaid
  `flowchart LR` text and a self-contained accessible SVG (`role="img"`, title, description with
  every box's text, dark ink on light fills) with alt text and a full text description. Writes a
  one-page Word (or PDF) table, the SVG, a PNG, and the Mermaid text to `/data/output`. The
  reusable diagram module is `crates/phn-tools/src/planning/diagram.rs`.
- `write_smartie_objectives` (PLAN-02): objectives written and checked against CDC's seven
  SMARTIE questions (specific, measurable, achievable, relevant, time-bound, inclusive,
  equitable). The structured checker assembles a statement from the parts (bracketed placeholders
  for what is missing), flags a missing baseline, target, unit, date, population, data source, or
  equity dimension, a target that moves the wrong way, and dates that cannot be read or fall
  before the start (years 1900 to 2100 only), and computes the change from baseline to target.
  Healthy People 2030 lookup by topic, keyword, or objective ID: national baseline with year,
  target, desired direction, status, most recent value, data source, and Leading Health Indicator
  flag, read live from the objective's ODPHP page or from the bundled snapshot
  `healthy-people-2030-objectives` (507 objectives: 356 core, 110 developmental, 40 research),
  built by `xtask/scripts/build_design_snapshots.py` (with `--check`). A local target is compared
  with the national one only in the same kind of unit. Optional Word worksheet.
- `build_evaluation_plan` (PLAN-05): an evaluation plan on the six steps of CDC's Program
  Evaluation Framework (2024): purpose, users, uses, evaluation types (formative, process,
  outcome, impact, economic) chosen from the program's stage, an evaluation matrix (question,
  indicator, measure, baseline, data source, method, timing, responsible role) drafted from a
  logic model or the nurse's own questions, with CDC's MMWR Table 5 example questions used word
  for word and every tool-written cell marked as a suggestion, an analysis plan, a use and
  dissemination plan, a dated or weekly timeline, CDC's cross-cutting actions and standards, and an
  optional RE-AIM lens. Gap checks (no indicator or data source, yes/no questions, no breakdown by
  group, impact evaluation without a comparison). Word (or PDF) plan with blanks; an optional
  staff roster adds names beside roles in the file only.
- Healthy People 2030 adapter (`phn-data`), toolset `planning`, three content files in
  `content/planning/` with schemas, and a bundled snapshot of 507 objectives (620 KB).

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Build a logic model for our postpartum home visiting program for first-time parents in Springfield. We have two nurses and a hospital referral partnership, and we visit within 14 days of discharge." | `build_logic_model` | The assistant drafts boxes from your words and calls the tool; the result has the table of components, if-then statements, and findings (for example a missing assumption or stage if you did not give one); Mermaid text; an SVG and PNG in `output/logic_model_…/`. Open the PNG: columns read left to right with arrows, text fits in the boxes, groups are labeled "Planned work" and "Intended results". Open the Word table: one portrait page. |
| 2 | "Here is our falls prevention description: [paste]. Turn it into a logic model and tell me what is missing." | `build_logic_model` | Outcomes written as activities ("Provide a class") are flagged as needs work; an outcome that does not say what changes is flagged; the response says it cannot judge whether the logic is sound. |
| 3 | "Paste the Mermaid text into a Mermaid viewer." | `build_logic_model` | The diagram renders (columns as subgraphs, arrows between them). Record the viewer used. |
| 4 | "Check this objective: by December 2027, raise postpartum visit attendance from 62% to 85% for first-time parents who deliver at Baystate." | `write_smartie_objectives` | The statement is rebuilt from parts; data source, equity dimension, and review points are flagged as missing; the change reads "23 percentage points (+37.1% of the baseline)"; achievable, relevant, inclusive, and equitable are marked as your judgment. |
| 5 | "Write three SMARTIE objectives for our diabetes program and link them to Healthy People." | `write_smartie_objectives` | Objectives have bracketed blanks where the assistant did not know a value (it should ask you); Healthy People objective D-04 shows national baseline 64.8% (2019) and target 70.3%, labeled live or snapshot with its date, with the age-adjusted note. |
| 6 | "What are the Healthy People 2030 objectives for adult tobacco use and flu vaccination?" | `write_smartie_objectives` | TU-01 and IID-09 (flu vaccine is a Leading Health Indicator) with baseline, target, direction, and status. Turn the internet off: the answer says it used the snapshot read 2026-11-12. |
| 7 | "My target is 40% but I want to decrease the rate. Check it." | `write_smartie_objectives` | A target above the baseline with "decrease" is flagged as moving the wrong way. |
| 8 | "Draft an evaluation plan for the postpartum program from this logic model, for a state grant. We are in the implementation stage; break results down by language." | `build_evaluation_plan` | Process and formative types; the matrix has CDC's example question on resources used word for word and labeled; rows drafted from each activity and output; suggested cells are listed per row; the Word plan has Steps 1 to 6 with blanks, a weekly timeline, and a note on small numbers. |
| 9 | "Add RE-AIM and dates from July 1, 2026 to June 30, 2027." | `build_evaluation_plan` | Five RE-AIM rows with the program and population filled in; dated phases that run end to end; the RE-AIM caveat about pragmatic use. |
| 10 | "Here are three evaluation questions I wrote, fill in the rest." | `build_evaluation_plan` | Your wording is kept ("your question"); a yes/no question is flagged; missing indicators show in brackets. |
| 11 | "Use staff_roster.csv so the plan shows who is responsible; our data person is the Incident Commander." | `build_evaluation_plan` | The chat shows only counts; open the Word plan: the name appears beside the role; no phone numbers or emails anywhere. Then try `line_list.csv` as the roster: nothing from it is shown. |

## Content review

- `content/planning/logic_model.yaml`: check the component definitions against CDC's Step 2
  page and Action Guide (high confidence; text quoted). **The W.K. Kellogg Foundation's Logic Model
  Development Guide could not be fetched** (wkkf.org, EPA's and NACCHO's copies are blocked), so
  nothing is attributed to it. Medium: the label "assumptions" (CDC asks for the rationale and
  evidence behind the links) and the Outputs column (CDC puts outputs with the evaluation
  measures). Low (tool defaults): six boxes per column, 25 words per box, and the word lists the
  wording checks use. Decide whether you want the heuristics at all.
- `content/planning/smartie.yaml`: the seven questions and three examples are quoted from CDC's
  2021 sheet "Writing Effective Objectives" (high). CDC's sheet says "attainable" and
  "time-based"; the tool says achievable and time-bound. The equity question is generalized from
  "cancer screening disparities" (flagged in the tool output). The originator of SMART and SMARTIE
  was not established and no citation is given (Doran 1981 could not be checked). Medium/low: which
  fields each element needs, the date formats, the equity-dimension prompts (examples only, from
  the Healthy People disparities page plus this project's list), the statement template.
- `content/planning/evaluation_framework.yaml`: steps, cross-cutting actions, standards,
  evaluation types, designs, methods, data-quality dimensions, question characteristics, the
  MMWR Table 5 questions, and the matrix columns are from CDC pages and the December 2025 Action
  Guide (high). The RE-AIM definitions are paraphrased from the 2019 open-access review (high for
  the definitions; re-aim.org was unreachable); the mapping of RE-AIM dimensions to process or
  outcome is this project's (medium). Low (tool defaults, marked in the file): the question,
  indicator, and design-by-type templates, data source and method suggestions, timing by type,
  default role names, the stage-to-type mapping (derived from CDC's "when to use it" column), and
  the six-step timeline shares.
- `snapshots/healthy_people_2030_objectives.json`: read from odphp.health.gov on 2026-11-12;
  507 objectives found through the 62 topic pages (the official total may be slightly different;
  one objective, PHI-D02, has no status on its page and is labeled `unknown`). Spot-check five
  objectives you use against their pages. Values change as ODPHP releases data.

## Open items

- **Healthy People API:** ODPHP has no public JSON API for the objectives that could be found
  (`odphp.health.gov/healthypeople/api` returns 404; `health.gov` is blocked by the proxy). The
  adapter and the snapshot builder read each objective's server-rendered page; a site redesign
  will break both (the snapshot keeps working; the builder and the live parser have tests and a
  fixture). The site shows a banner that it was restored to its January 29, 2025 version under a
  court order, so content can change; the tool says to re-check pages before citing.
- Subpopulation (disparities) data and state data on the Healthy People pages are not included.
  Add a disparities view if the nurse wants to set equity targets from national gaps.
- The snapshot is 620 KB in the binary; consider shrinking (drop `most_recent_text` duplicates)
  if image size matters.
- Decisions for the nurse: whether to keep the Outputs column in the default logic model
  (CDC's current guide folds outputs into the evaluation measures); the 6-box and 25-word limits;
  whether SMARTIE should use "achievable" (the project's term) or CDC's "attainable"; whether the
  evaluation plan should default to a 52-week timeline; whether to add `economic` questions beyond
  the single CDC example; a landscape page option for the Word logic model (phn-docs has portrait
  only; the diagram is therefore not embedded in the Word file).
- PLAN-01 and PLAN-05 take the model's own drafted components; the tools do not parse a
  free-text program description. A document-reading path (policy or grant narrative in `/data`)
  would need redaction rules and a leak-test row.
- Evaluation-plan roles match a roster only when exactly one person holds the position (exact
  normalized position match); a name is never inferred.
- Network lessons: www.cdc.gov (evaluation pages and the Action Guide PDF), stacks.cdc.gov (PDF
  and record page), odphp.health.gov (topic and objective pages), and eutils.ncbi.nlm.nih.gov
  (PubMed and PMC) are reachable; health.gov, wkkf.org, re-aim.org, api.crossref.org,
  pubmed.ncbi.nlm.nih.gov, oregon.gov, naccho.org, epa snapshot hosts, and the old CDC SMARTIE PDF
  path (`cdc.gov/cancer/ncccp/pdf/smartie-objectives-508.pdf`, now 404) are not. CDC retired its
  1999 framework pages; the 2024 framework uses "interest holders", three cross-cutting actions,
  five standards, and six steps (Assess the context is new).
