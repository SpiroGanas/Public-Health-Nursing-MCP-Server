## Changelog

- `find_evidence_based_interventions` (PLAN-03): Community Guide evidence for a health topic, with
  the evidence wording each paper's abstract uses, the source and date, short excerpts, and any
  implementation, barrier, or cost sentence. The records are the journal papers PubMed attributes
  to the Community Preventive Services Task Force (systematic reviews, economic reviews, and Task
  Force recommendation statements): bundled snapshot `cpstf-publications` (145 papers, 2000 to
  2026), built by `xtask/scripts/build_interventions_snapshots.py` (with `--check`) from one
  PubMed search and one efetch. A rating (strong, sufficient, or insufficient evidence; the Task
  Force recommends or recommends against) is shown only when the abstract's own words give it, and
  the result says when the PubMed record gives none; the Rust reader of those words is tested
  against every sentence the builder stored. Topic words are expanded through 26 topic groups
  (smoking finds tobacco), ranked by title, MeSH headings, and abstract excerpts, and can be
  narrowed by population, setting, kind of paper, and evidence wording. When online it also
  searches PubMed for newer papers (only the topic words are sent) and falls back to the snapshot
  on any failure, saying which was used. Papers eight or more years old carry a reminder to check
  for updates. The Community Guide and What Works for Health sites were blocked from the build
  network, so What Works for Health has no data and the result says what to check on each site.
- `apply_behavior_change_theory` (PLAN-04): maps a program's problem, population, setting,
  barriers, and planned components onto 48 constructs of eight theories and frameworks (Health
  Belief Model, Social Cognitive Theory, Theory of Planned Behavior, Transtheoretical Model,
  Socio-Ecological Model, Diffusion of Innovations, COM-B with the Behaviour Change Wheel, and
  Self-Determination Theory), with the strategy and measure ideas for each construct. Constructs
  are paraphrased from nine open-access papers read through PubMed Central and CDC's
  social-ecological model page; 25 of the 62 strategies come from a published table (the Social
  Cognitive Theory construct-to-technique table, the Behaviour Change Wheel's links, CDC's
  examples) and the rest are marked as the project's own suggestions. Barriers are matched to
  constructs by whole-word signals and planned components to strategies that share a word; the
  tool picks up to three theories that match (adding the Socio-Ecological Model when none looks
  beyond the individual) or uses the ones named. Text with identifiers is refused. Optional Word
  or PDF worksheet.
- `track_program_performance` (PLAN-11): reads a program's results by period from a local
  CSV/XLSX (one row per period, one row per period and measure, or one row per record counted by
  month, quarter, or week) and compares them with the nurse's targets: cumulative totals and
  pooled percentages, progress to target (with an optional baseline), pace against the program
  year (met, on track, behind), and run-chart or p-chart trend flags. Only aggregates by period
  are returned; a count from 1 to the threshold minus 1 is hidden and then the totals, progress,
  center line, and status that would reveal it are withheld. Writes a dashboard report (Word or
  PDF), a workbook with live formulas (Summary and Data sheets), and a chart per measure to
  `/data/output`, with the full values from the nurse's own file. Only the named columns are read:
  identifier columns are refused and other columns (for example contact people) are never read.

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "What does the Community Guide say works to increase colorectal cancer screening?" | `find_evidence_based_interventions` | Papers on client reminders, patient navigation, community health workers, and the economic reviews, each with its journal, date, and PubMed link. A paper's rating wording matches its abstract on PubMed (open two). Papers without rating words say "check the Community Guide page". The result says it is not the Community Guide's own list, that What Works for Health is not included, and whether PubMed was live or the snapshot (with its date). |
| 2 | "Find evidence-based interventions to reduce tobacco use among adolescents in schools." | `find_evidence_based_interventions` (`population` adolescents, `setting` school) | Recognizes tobacco and smoking as one topic; smokefree policy and tobacco papers rank with school and youth papers boosted; old papers (2001 and 2010) carry the check-for-updates reminder. |
| 3 | "What interventions have insufficient evidence for obesity?" | `find_evidence_based_interventions` (`evidence` insufficient) | Only papers whose abstracts say insufficient evidence (for example school nutrition and activity); the caveat says insufficient evidence is not evidence of no effect. |
| 4 | "Find evidence for a topic with no Community Guide paper, such as lead paint abatement." | `find_evidence_based_interventions` | No matches, with the caveat to try other words or search the Community Guide and What Works for Health directly; nothing invented. |
| 5 | Open Community Guide and What Works for Health yourself for two topics from the results. | (check) | Compare the Task Force's current finding with the tool's wording; record any paper whose recommendation was updated or archived. |
| 6 | "Which behavior change theories fit low mammography screening among Portuguese-speaking women over 50? Barriers: cost, no transportation, family does not talk about it, forgot to book. Planned: text reminders, peer navigators." | `apply_behavior_change_theory` | Up to three theories (for example HBM, TPB, SCT) plus the Socio-Ecological Model; each barrier maps to constructs (cost and transportation to perceived barriers; family to subjective norm or interpersonal; forgot to book to cues to action); text reminders map to cues to action; strategies are marked "from" a source or "tool's suggestion". Read two definitions against the cited paper. |
| 7 | "Show me COM-B and the Socio-Ecological Model for a school vaping prevention program." | `apply_behavior_change_theory` (`theories` comb, sem) | Six COM-B components with intervention types from the Behaviour Change Wheel; five SEM levels with CDC's examples marked as from CDC. |
| 8 | "Apply behavior change theory to: Maria Garcia, 837 Elm Street, skips her visits." | `apply_behavior_change_theory` | Refused: describe a population, not a person; the name and address are not repeated. |
| 9 | "Make a worksheet of all theories for our flu vaccine clinic plan." | `apply_behavior_change_theory` (`theories` all, `write_document` true) | A Word file in `/data/output` with a table per theory; open it and check it reads well and says strategies marked as the tool's suggestion are drafts. |
| 10 | "Track my program in program_metrics.csv. Period column Month, 12 months in the year. Clients enrolled target 400; screening rate is Participants screened over Participants eligible, target 80 percent; sessions held target 130; housing referrals target 60." | `track_program_performance` | Clients enrolled 340 of 400 (85.0 percent, expected 400 by now, Behind); screening 76.8 percent pooled (not 76.2, the mean of months), Below target; sessions held 120 of 130, On track, with a run-chart signal for the steady rise; housing referrals "Not shown" because five months have counts under 5, with totals withheld. No contact person, phone, or email appears in the chat. Open the workbook in Excel and check the Summary formulas recalculate when you add a row to Data, and that the report shows the hidden counts. |
| 11 | "Compare program_outcomes_long.csv with targets: controlled blood pressure 70 percent, home visits 240. Quarters in the year: 4. Period column Quarter, outcome column Outcome, value column Value, denominator column Denominator." | `track_program_performance` | Quarters sort 2025 Q3 to 2026 Q2; blood pressure 70.4 percent pooled, Met; home visits 215 of 240, Behind (215 is just under 90 percent of the pace needed). Staff names and emails in the unread columns do not appear. |
| 12 | "Count the cases in line_list.csv by month and tell me whether there is a shift." | `track_program_performance` (`count_rows`) | Counts by month only (no dates, names, or case IDs); small counts hidden; with under six months, no trend rules and a note saying so. |
| 13 | "Track line_list.csv by Last Name." | `track_program_performance` | Refused: the column cannot be analyzed; no name appears. |

## Content review

- `content/planning/behavior_theories.yaml` (eight theories, 48 constructs, 62 strategies;
  `status: pending`). Constructs and definitions are paraphrased from nine open-access reviews
  (full text read through PubMed Central) and CDC's social-ecological model page. The National
  Cancer Institute's Theory at a Glance and the original theory papers (Rosenstock, Ajzen,
  Bandura, Prochaska, Rogers, McLeroy) were not reachable, so check each definition there before
  it goes into a grant or paper. Check especially: (1) the Socio-Ecological Model's levels (the
  reviews use five, CDC's page four, and the file uses the five with CDC's examples on four of
  them); (2) the Diffusion of Innovations entry (only the five innovation attributes and the
  opinion leader, change agent, and gatekeeper roles were in a source read; Rogers' adopter
  categories are not included); (3) the 37 strategies marked "authored" (this project's plain
  language suggestions, medium to low confidence, for example the stage-matched strategies for the
  Transtheoretical Model, which no source linked to stages); (4) every measure idea (indicator
  ideas, not validated instruments; no survey instrument is copied); (5) the "signals" lists that
  match a nurse's words to constructs (a matching aid; add the words your barriers use). Not
  covered because no source was read: the Health Action Process Approach, the
  Information-Motivation-Behavioral Skills model, the Extended Parallel Process Model, social
  marketing, community readiness, and intervention mapping.
- `content/planning/evidence_sources.yaml` (`status: pending`): the rating key (the Community
  Guide's exact definitions of strong and sufficient evidence were not read; the wording comes from
  sentences in the abstracts), 26 topic groups with synonyms (this project's search aid, chosen
  from the titles and MeSH headings of the papers in the snapshot), and the notes on the two sites
  that could not be read. Confirm that the Community Guide and County Health Rankings home page
  addresses are right (they were not opened).
- `snapshots/cpstf_publications.json` (145 papers published 2000 to 2026, retrieved 2026-11-12):
  titles, journals, dates, DOIs, and MeSH headings are PubMed's. Check: open five papers on
  PubMed and compare the rating sentences and the finding, conclusion, and implementation
  excerpts with the abstract; check the 51 papers typed "recommendation" from their titles (a few
  are combined reports, not Task Force statements) and the six that carry a "title signal" of
  recommended or recommended against (five and one); and compare a handful of the Task Force's current findings
  on the Community Guide site, because a paper can be older than the current recommendation. The
  snapshot is not the Community Guide's list; many recommendations have no journal paper. Abstract
  excerpts are short quotations; abstracts can be copyrighted by the journal.

## Open items

- **Blocked sources.** `www.thecommunityguide.org` and `www.countyhealthrankings.org` (and
  `cancercontrol.cancer.gov`, NCI's Theory at a Glance) answered the connection request with HTTP
  403 from the build network. Their terms of use were therefore not read. What Works for Health
  has no data at all; if the nurse can reach either site, the next step is a snapshot of its
  strategy ratings (summarized and linked, not copied) with the site's terms checked first. The
  Healthy People 2030 adapter belongs to the design package and was not built here.
  `www.cdc.gov` pages for HI-5 are gone (404 or 403), and www.ncbi.nlm.nih.gov web pages, Europe
  PMC, Crossref, and doi.org are not reachable; NCBI E-utilities (PubMed and PMC full text for
  open-access papers) are, and were the way to read theory papers.
- **Evidence tool limits.** Ratings are per paper, not per intervention, and appear only when the
  abstract (or, for six, the title) says them: 49 of 145 papers have rating words in the abstract
  and 41 have no abstract in PubMed. A paper can be updated or archived after
  publication, and recommendations with no journal paper are missing. The live search only finds
  papers PubMed attributes to the Task Force by Corporate Author, and live records carry no MeSH
  headings, so they rank on title and abstract words only. The snapshot should be rebuilt
  (`python3 -I xtask/scripts/build_interventions_snapshots.py`, `--check` in CI if network is
  allowed) when new Community Guide papers appear; the weekly snapshot check in CI does not cover
  it yet.
- **Theory tool limits.** Matching is keyword based (whole words and plural forms), so a barrier
  written in unusual words may match nothing; the nurse's assessment data should drive which
  constructs to target. The library is eight theories; adding one means a YAML entry with sources
  read. Strategies marked "authored" are the project's own and need the nurse's judgment.
- **Performance tool limits.** Two layouts (wide and long) plus row counting; no formulas or
  merged cells in the input file, one measure per column or per measure value. A percentage is
  pooled across periods; "on track" at 90 percent of an even pace is this project's convention
  (`on_track_fraction`); no seasonal or phase-change handling in the trend rules (use
  `create_spc_chart` for phases). The workbook formulas cover Data rows 2 to 101 (100 periods) and
  were checked by recalculating the workbook in LibreOffice against the tool's own numbers, not in
  Excel. Suppression hides counts of 1 to threshold minus 1; the nurse's report and workbook keep
  the full values (they are aggregates by period written to `/data/output`). A file with one
  measure value per site per period is summed (counts) or pooled (percentages).
- **Decisions for the nurse.** Whether the "on track" convention matches your funders' rule;
  whether to keep all eight theories or trim to those you use; whether the evidence tool should
  list only papers with an explicit rating by default (today it lists all and labels each).
- **Lessons.** PubMed Corporate Author searches are a workable way to read an organization's
  publications when its site is blocked (the USPSTF and CPSTF snapshots both use it); PMC
  `efetch db=pmc` returns full text for open-access articles, which is how theory papers were
  read; NCBI Bookshelf text is not reachable. Cite theories by journal and year in fixed output
  (leak tests seed common surnames). rust_xlsxwriter's `Formula::set_result` stores the cached
  value so a viewer that does not recalculate still shows numbers.
