# Fragment: Phase 15 `screen` (CHR-02, CHR-03)

## Changelog

- `community_bp_screening_toolkit` (CHR-02, toolset `chronic`): a community blood pressure
  screening kit for adults. It gives the measurement technique checklist (CDC's steps; two
  steps that are standard practice but not found at a source are marked "not verified"), the
  current ACC/AHA categories (the 2025 AHA/ACC guideline retired and replaced the 2017 one;
  each category carries its source and confidence), the severe-range wording and CDC's
  pregnancy thresholds, referral tiers labelled "tool default, pending review", a screening log
  template (XLSX and CSV), three follow-up letter templates, a reference card, and a
  plain-language client handout (reading grade checked with the COMM-01 engine), written to
  `/data/output`. It classifies typed readings (averaged; any severe reading flagged). A local
  screening log is summarized only as suppressed one-way counts (category, age group, referral
  status, readings used), and follow-up letters filled from its rows go to `/data/output` only.
- `diabetes_prevention_finder` (CHR-03, toolset `chronic`): scores the CDC Prediabetes Risk Test
  from CDC's published points, checks CDC's eligibility rules for the National Diabetes
  Prevention Program lifestyle change program, and lists the Massachusetts sites on DPH's
  "Active MA DPPs" list for a town (and its county), county, or the state from a bundled
  snapshot (`ma-dpp-programs`, 73 sites, retrieved 2026-11-12; builder
  `xtask/scripts/build_screen_snapshots.py` with `--check`). It explains CDC's delivery modes
  and how to find online options, gives Medicare Diabetes Prevention Program facts from CMS,
  writes an optional plain-language handout, and summarizes a screening-event file as
  suppressed counts. It states plainly that CDC's registry could not be read (so no
  recognition status, delivery mode, or online providers) and makes no MassHealth statement.

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Make a blood pressure screening kit for our health fair at the senior center." | `community_bp_screening_toolkit` | Folder `bp_screening_kit` in `/data/output` with the log template (XLSX and CSV), checklist, reference card, three letters, and a handout; every referral tier says "tool default, pending review"; the categories show their sources and confidence; the answer says the 2025 guideline replaced the 2017 one and that its full text was not read; the XLSX opens with column headers. Check the handout and letters read plainly. |
| 2 | "Two readings were 146/92 and 142/90 on a 62-year-old. What category is that and what do I do?" | `community_bp_screening_toolkit` (`readings`) | Average 144/91, stage 2 range; referral tier "refer" labelled as a tool default; no kit files written; reminds that one screening cannot diagnose. |
| 3 | "A reading was 192/114 and she has chest pain." | `community_bp_screening_toolkit` (`readings`, `symptoms` true) | Severe range, emergency tier (call 911), flagged as a tool default; the symptom list is marked low confidence. With `symptoms` false: same-day tier. |
| 4 | "A pregnant client reads 162/104." | `community_bp_screening_toolkit` (`readings`, `pregnant` true) | Uses CDC's pregnancy thresholds (160/110 severe; 140/90 high), not the adult categories; same-day tier. |
| 5 | "Summarize bp_screening_log.csv and write follow-up letters." | `community_bp_screening_toolkit` (`log_path`, `write_letters`) | Counts by category, age group, referral status, and readings used; small counts shown as `*` with at least two hidden per table; no names, addresses, phone numbers, or readings appear in the chat; one DOCX of letters in `/data/output` (30 letters; the 15-year-old and the row with no reading get none); the right letter wording by category. |
| 6 | "Summarize line_list.csv as a screening log." | `community_bp_screening_toolkit` (`log_path`) | Refused because the file has no blood pressure columns; no values from the file are shown. |
| 7 | "A 54-year-old man, 5 feet 10, 240 pounds, family history, high blood pressure, not active. What is his Prediabetes Risk Test score, and are there programs in Fall River?" | `diabetes_prevention_finder` | Score 8 of 10 with points for each factor (age 2, man 1, family 1, high blood pressure 1, inactive 1, BMI 34.4 = 2); high risk but not a diagnosis; four Fall River sites with addresses; no coordinator names or emails; says the list is DPH's, undated, and not CDC's registry. |
| 8 | "Is a 45-year-old Asian American woman with a BMI of 24 and past gestational diabetes eligible for the lifestyle change program?" | `diabetes_prevention_finder` | Score 3 (woman with gestational diabetes 1, age 1, BMI 24 at the Asian American cut point 1); meets the BMI rule (23) and the gestational diabetes route; says whether diabetes and pregnancy were given. |
| 9 | "Where can my client find an online diabetes prevention program, and does Medicare pay?" | `diabetes_prevention_finder` | Explains in-person, live virtual, and on-demand modes (CDC) and how to filter on CDC's page; does not name online providers; Medicare Part B coverage, 16 weekly and 6 monthly sessions, up to 22, online delivery through 2029 (CMS); says MDPP eligibility was not checked and gives no MassHealth coverage statement. |
| 10 | "Write a handout about prediabetes for a client with a score of 6." | `diabetes_prevention_finder` (`handout` true) | DOCX in `/data/output`; reading grade 8 or lower shown; the score filled in; blank line for the program phone if none is saved. |
| 11 | "Count how many people at the fair scored high risk in prediabetes_responses.csv." | `diabetes_prevention_finder` (`responses_path`) | Suppressed counts by result and age group; no names or phone numbers; a row missing an answer counts as "could not be scored". |

## Content review

- `content/chronic/bp_screening.yaml` (status `pending`, review due 2027-02-01). Check against the
  2025 AHA/ACC guideline text (which this build could not read: ahajournals.org, heart.org,
  acc.org, nhlbi.nih.gov, and PMC web pages were blocked):
  - the stage 1 (130-139 or 80-89), stage 2 (140 or higher, or 90 or higher), and severe-range
    (above 180 or above 120) wording; **medium confidence**: taken from open-access peer-reviewed
    papers that state them (PMC13732403, PMC13683027, PMC12995957), not from the guideline;
  - normal, elevated, 130/80, the technique steps, and the pregnancy thresholds (140/90; 160/110)
    come from CDC pages updated 2026-10-13 (**high**);
  - corrections to the guideline were published in Circulation and Hypertension in 2026 and were
    not reviewed;
  - **low confidence, this project's drafts:** all five referral tiers and the any-reading
    severe rule, the emergency symptom list, the cuff-size and record-keeping technique steps
    (marked "not verified"), the log columns and referral status words, the three letters, the
    handout, and the plausibility limits for readings (systolic 50-300, diastolic 30-200). Decide the referral timing and symptom list
    with your medical director or standing orders.
- `content/chronic/diabetes_prevention.yaml` (status `pending`, review due 2027-02-01): the test
  points, 5-of-10 cut-off, BMI example, eligibility, program schedule, delivery modes, and
  Medicare facts were read on CDC and CMS pages on 2026-11-12 (**high**). The questions are
  paraphrased because the test was developed with the American Diabetes Association; confirm reuse
  terms for the full test. The tool's BMI cut points use a BMI rounded to one decimal; CDC's
  online test may use a height and weight table (unverified). MDPP eligibility was not read.
  MassHealth coverage: no source found, none stated. The handout is a project draft (**medium**).
- Snapshot `ma-dpp-programs` (`snapshots/ma_dpp_programs.csv`): DPH's "Active MA DPPs" table as
  retrieved 2026-11-12. The page shows no date; sites may have closed; CDC recognition status and
  delivery mode are not in it. One ZIP code (Lanesborough, 02137) looks wrong on DPH's page and
  is kept as published. Individual contact names, emails, and phone numbers (56 of 73 sites) were
  dropped on purpose; the page's reuse terms (a state government page) were not confirmed.

## Open items

- **CDC's DPP registry was not reachable** (the plan's National DPP registry snapshot): the proxy
  refused `dprp.cdc.gov` and `dprpdataportal.cdc.gov`; `nccd.cdc.gov` is reachable but the
  old `/DDT_DPRP/` application returns 404 (no ASP.NET form to post to). The CDC "Find a
  Program" page embeds `dprpdataportal.cdc.gov/programlocator/`. If those hosts are allowed
  later, rebuild the snapshot from the registry (which has recognition status, delivery mode,
  and organization contacts) and drop the DPH-list caveats. Until then the tool lists no online or
  distance-learning providers.
- **No MassHealth statement** (no source found); **no MDPP eligibility check** (not read).
- The log summary cannot cross two variables (for example category by referral status) because
  small cells could then be recovered between tables; a future version could suppress tables
  jointly. Letters are English only (agency translations via `/data/templates` are not wired in
  for these letters); no pregnancy column in a log; a log's category column is ignored (the tool
  classifies from the numbers).
- Decisions for the nurse: the referral tiers and timing; whether severe range should trigger on
  any single reading (the tool default) or only on the average; whether the screening letters
  should name the reading.
- Network lessons: ahajournals.org, professional.heart.org, acc.org, heart.org, nhlbi.nih.gov,
  medlineplus.gov, europepmc.org, ncbi.nlm.nih.gov web pages (including PMC and Bookshelf),
  data.cdc.gov catalog search via api.us.socrata.com, dprp.cdc.gov, and dprpdataportal.cdc.gov
  were blocked. Reachable: www.cdc.gov (the high blood pressure and diabetes prevention pages),
  www.cms.gov, www.mass.gov (plain `curl` or the project's user agent), and NCBI E-utilities
  (`efetch db=pmc` returns open-access full text, which is how the 2025 guideline was checked
  through peer-reviewed secondary sources). PubMed's record for the 2025 guideline says it
  "retires and replaces" the 2017 guideline; a rate limit (HTTP 429) appears after a few
  E-utilities calls in a row.
- The CDC Prediabetes Risk Test question wording is not on CDC's scoring page; only the points
  and two sample questions are. The tool paraphrases.
