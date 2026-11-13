# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/). Version numbers track the phases in
[`project_plan.md`](project_plan.md).

No version has been tagged or published as a release yet. The date on each version is the day
it was merged to `main` (Eastern time).

## [Unreleased]

## [1.0.0] — 2026-11-13 (not yet tagged) — Phase 19: policy and law; v1.0 hardening

The last phase of [`project_plan.md`](project_plan.md): five policy tools and the v1.0 hardening
work. The server now has all 148 planned tools.

### Added

- Policy tools (toolset `policy`):
  - `track_ma_legislation` (POL-01): tracks Massachusetts bills of the 194th General Court
    (2025-2026) on 18 public health topics (or your title keywords, bill numbers, or a watchlist
    CSV) from the Legislature's public API (`malegislature.gov/api`, no key). For each bill it
    gives the stage (this project's reading of the history, by rules shown in the content file),
    the latest committee referral, hearing lines with the date and days until it, the committee's
    reporting deadline, the last action with its date, and an excerpt of the bill text of 25 words
    or fewer with its citation, plus structured questions for the nurse's own analysis (it never
    says what a bill would do). It can save a watchlist to `/data/output`. Live (the 12 MB bill list
    is cached for a day), then a dated bundled snapshot (822 bills, built 2026-11-13); it says which
    was used. Sponsor and amendment-author names are not returned.
  - `monitor_federal_rules` (POL-03): watches the Federal Register API for proposed rules, final
    rules, and notices from CDC, CMS, HRSA, EPA, USDA, and HHS (also FDA, SAMHSA, ACF, FNS, FSIS,
    or any agency slug), with each comment deadline counted down against today's date
    (golden-tested against Python date arithmetic), an optional Regulations.gov cross-check
    (`PHN_REGULATIONS_GOV_API_KEY`, never shown), and a nine-section public comment outline (Word,
    `[NURSE: ...]` placeholders, filled from a field/value CSV, built from EPA's published tips for
    effective comments, quotations of 25 words or fewer). Live only: when the Federal Register
    cannot be reached it says LIVE DATA UNAVAILABLE and lists nothing. The Federal Register and
    Regulations.gov adapters were never run against the live services (both hosts were blocked
    from the build environment); they were tested against local mock servers, and `#[ignore]`d live
    tests exist.
  - `search_public_health_law` (POL-02): searches a bundled, dated (2026-11-12) corpus of
    Massachusetts public health law and answers with citations. M.G.L. c. 111: a section index of
    all 510 sections that are not repealed and full text for 45 core sections. 105 CMR: all 89
    chapters with page dates and links, a 3,060-section number and title index, and full text for
    20 chapters. 244 CMR: all eight chapters in full text (51 sections). A citation returns the
    exact section or the chapter's sections; words are matched on titles and text. Every hit has
    the citation, title, link, chapter date, snapshot date, whether the section is `full_text` or
    `index_only`, and an excerpt of 25 words or fewer (machine-checked). A folder of your board's
    own regulations under the data folder (default `local_regulations`) is indexed with the Phase 1
    policy index (files with a date of birth, record number, or health plan number are refused).
    An optional larger corpus (`law_corpus/` in the data or cache folder, built with
    `xtask/scripts/build_law_snapshots.py --extended`) is used only when its checksums match.
    Decision support, not legal advice.
  - `draft_policy_brief` (POL-04): drafts a one-page or full policy brief (DOCX and/or Markdown in
    `/data/output`) in the order of the Eightfold Path of policy analysis, with ratings in words
    (strong, mixed, weak, unknown) so the options-by-criteria table reads without color. The nurse
    supplies the problem, evidence with citations, options, criteria, ratings, and recommendation;
    everything else is a `[NURSE: ...]` placeholder, and the tool never invents facts. It counts the
    placeholders left and runs the readability analysis on only the prose the nurse supplied.
  - `health_information_sharing_guide` (POL-07): asks five structured questions (who holds the
    record and in what role, the kind of record, the recipient, the purpose, consent) and returns
    the regimes that may govern (HIPAA, FERPA, 42 CFR Part 2, the Massachusetts public records law,
    105 CMR 300.120 and 300.191, and state confidentiality statutes), ordered likely governs, may
    apply, check, or not indicated, each with provisions, a 25-word machine-checked quotation, and
    cautions. It never gives a yes or no on whether a disclosure is allowed and refers to agency
    counsel.
- Content drafted from the Legislature, mass.gov, the eCFR, EPA, and open-access articles, all
  pending review: `content/policy/ma_legislation_tracking.yaml`, `federal_rules_tracking.yaml`,
  `public_law_search.yaml`, `policy_brief.yaml`, and `information_sharing.yaml`, with schemas.
- Snapshots, each with a builder in `xtask/scripts/` (with a `--check` mode): `ma-legislation-194`
  (822 bills), `ma-law-mgl-c111`, `ma-law-105-cmr`, and `ma-law-244-cmr`.
- Synthetic fixtures; every new data tool is in the leak test.
- Acceptance script `docs/acceptance/phase-19.md` (including the v1.0 hardening checks) and user
  guide chapter 22.
- Hardening: see `docs/security/threat-model.md`, `docs/admin-guide.md`
- A final content review sweep for the nurse, `docs/content-review-sweep.md`, listing every content
  file and snapshot added in Phases 15 to 19 with its confidence counts and what to verify.
- User guide: chapters 18 to 22 (one for each of Phases 15 to 19) and front matter for all 148
  tools. The Word file is rebuilt with `node docs/user-guide/build-user-guide.js` at release.

### Changed

- MCP prompts: the Phase 15 `community_health_assessment` and the Phase 17
  `prepare_grant_application` prompts are now in the prompt list (four prompts in all, with
  `investigate_outbreak` and `plan_tabletop_exercise`); the unknown-prompt error message names all
  four, and the prompt-list test now asserts the exact list in order instead of checking only the
  first two.
- `phn-data`: `HttpClient::post_json_bytes` (added in Phase 17 for the grants search) is part of
  the shared client.
- `phn-tools`: `ebp/spc.rs` now exposes `Prepared` and `prepare_file` as `pub(crate)` so the QI
  tool reuses its event-file grouping and suppression (Phase 16; no behavior change).
- `analyze_health_department_budget` accepts a ledger column whose only privacy finding is an
  "account" or "acct" header word (`ledger_column` in `crates/phn-tools/src/leadership/budget.rs`);
  its values are still scanned for identifiers and account codes are never returned. Any other
  finding still refuses the column. The PHI detector's header rule, which flags any header with
  "account", is unchanged. Needs the nurse's confirmation (see `STATUS.md`).
- The offline geocoder keeps its street-type vocabulary in a new folder, `crates/phn-data/data/`
  (`geocoder_vocab.json`), shared by the Rust tool and the Python index builder (ADR 0011).

## [0.19.0] — 2026-11-13 (not yet tagged) — Phase 18: leadership, accreditation, and professional practice

### Added

- Leadership tools (toolset `leadership`):
  - `assess_foundational_public_health_services` (LEAD-03): a gap analysis against the
    Massachusetts local public health standards (G.L. c. 111, s. 27D, the State Action for Public
    Health Excellence law enacted in 2020) and the Foundational Public Health Services framework.
    Four sets are rated by the nurse (the tool never fills in an answer): DPH's Performance
    Standards (93 mandated local responsibilities from DPH's Excel workbook), DPH's workforce
    standards (6 positions and 7 inspection types), the 56 FPHS Headline Responsibilities, and the
    4 program terms in s. 27D(a) with no single FPHS counterpart. Without statuses it writes a
    self-assessment worksheet (XLSX) and guide (DOCX); with typed statuses or the filled worksheet
    it returns counts by set and area, a gap table, a three-tier priority view (legal mandates
    first; the nurse can override any item), and shared-service options (statute models, Public
    Health Excellence grants, training hubs, and the shared service arrangement DPH's April 2026
    list gives for the town), and writes the gap analysis (DOCX and XLSX).
  - `phab_accreditation_crosswalk` (LEAD-04): maps the nurse's own policies, plans, and reports to
    the 10 domains and 20 standards of PHAB Standards & Measures Version 2022 with the Phase 1
    document index: strong, some, or no evidence per standard, the documents (the nurse's file
    names), the cues matched, passages of at most 20 words with names and contact details
    redacted, gaps, and an optional measure-level crosswalk from a measures list in the nurse's own
    PHAB copy. Files that look like client records or interview transcripts are left out. PHAB's
    measure text is not bundled (copyright; the PHAB site was unreachable).
  - `build_strategic_plan` (LEAD-05): scaffolds a strategic plan from content the nurse supplies:
    SWOT or SOAR prompts beside the nurse's entries, a stakeholder power and interest grid
    (accessible SVG and PNG with a text key, plus a table), coded priorities, goals, objectives,
    and strategies, the link to the community health improvement plan (CHIP), a blank-field check,
    and a draft DOCX. PHAB's strategic plan requirements are reported as unverified.
  - `assess_workforce_competencies` (LEAD-06): self-assessments against the eight domains and three
    tiers of the Council on Linkages Core Competencies for Public Health Professionals (2021) or
    the Quad Council Community/Public Health Nursing Competencies (2018), on a four-level scale.
    With no files it writes a blank workbook (live formulas; team results hide any result for
    fewer than 3 people) and a Word guide. With a ratings file it returns team means by domain,
    gaps to a target, means by tier, and a ranked training plan. With a credentials file it tracks
    the Massachusetts local public health workforce standards from the bundled
    `ma-performance-standards` snapshot (for example MAVEN training within 6 months), by
    requirement and position, with the same small-group rule. Staff are identified by code; name,
    phone, and email columns are refused. Competency statements are not bundled (the frameworks'
    sites were blocked).
  - `board_of_health_meeting_assistant` (LEAD-07): `notice_check` computes the latest time to post a
    meeting notice under the Open Meeting Law (G.L. c. 30A s. 20(b), 940 CMR 29.03): 48 hours,
    leaving out whole Saturdays, Sundays, and Massachusetts legal holidays (bundled list for
    2026-2028), and checks a posting time, notice contents, topic specificity, remote access, and
    executive session items; `agenda`, `staff_report`, and `minutes` write Word drafts (minutes from
    the nurse's notes, checked against the parts s. 22(a) and 940 CMR 29.10-29.11 require; the notes
    are scanned for identifiers by line number and never returned); `oml_guide` returns the
    requirements with sources.
  - `draft_standing_orders` (LEAD-08): drafts a standing order or protocol skeleton (vaccine,
    medication, or activity) as a Word file for the medical director's review and signature,
    following Board of Registration in Nursing Advisory Ruling 9324 (ten required elements and three
    limits) and, for vaccines, Advisory Ruling 9804. The nurse's text goes in with the source she
    cites for each clinical section (required); everything else is a `[NURSE: ...]` or
    `[MEDICAL DIRECTOR: ...]` placeholder. No clinical content is bundled.
- Evidence-based practice tool (toolset `ebp`):
  - `qi_vs_research_determination` (EBP-07): a screening aid that sorts a project into quality
    improvement, program evaluation, public health practice or surveillance, or possible human
    subjects research. The nurse answers 14 questions drawn from 45 CFR 46.102 (including the 2018
    public health surveillance exclusion), CDC's 1999 guidelines for defining public health
    research and non-research, and two quality improvement ethics sources read as abstracts. The
    tool never answers for the nurse; the reading is provisional and the IRB decides. It writes a
    draft project description and IRB determination request (DOCX) to `/data/output`.
- Policy tools (toolset `policy`):
  - `public_health_ethics_analysis` (POL-05): structures an ethics analysis of quarantine or
    isolation, exclusion from school, allocation of scarce resources, or another decision with
    Kass's six questions, a table of options from least to most restrictive, a justification
    checklist, and the APHA Public Health Code of Ethics (2019) values and analysis steps, as a
    DOCX worksheet. It never recommends a decision and does not cover Massachusetts law.
  - `nursing_scope_of_practice_guide` (POL-06): finds the provisions that bear on a scope,
    delegation, or standing-order question in a partial snapshot (2026-11-12) of 244 CMR 3.00,
    9.00, and 10.00, parts of 244 CMR 4.00, M.G.L. c. 112, section 80B, the Board's decision-making
    guidelines, and eleven advisory rulings (59 provisions, each with a citation, a quotation of 25
    words or fewer machine-checked against the fetched text, and a confidence level; 23 other
    rulings appear by title only). It never says that an act is within a nurse's scope.
- Content drafted from DPH, the Legislature, the Board of Registration in Nursing, the Attorney
  General, the eCFR, CDC, and open-access articles, all pending review:
  `content/leadership/` (`fphs_framework.yaml`, `phab_standards.yaml`, `strategic_planning.yaml`,
  `workforce_competencies.yaml`, `open_meeting_law.yaml`, `ma_legal_holidays.yaml`,
  `standing_orders.yaml`), `content/ebp/qi_research_determination.yaml`, and
  `content/policy/public_health_ethics.yaml` and `nursing_scope_of_practice.yaml`, with schemas.
- Snapshots: DPH's Performance Standards (`ma-performance-standards`) and shared service
  arrangements for all 351 municipalities (`ma-shared-service-arrangements`), built by
  `xtask/scripts/build_strategy_snapshots.py` (with `--check`); the Massachusetts legal holiday list
  is generated by `xtask/scripts/build_workforce_holidays.py` (with `--check`).
- Synthetic fixtures with seeded identifiers (a filled FPHS worksheet, workforce ratings and
  credentials, board meeting notes); every new data tool is in the leak test.
- Acceptance script `docs/acceptance/phase-18.md` and user guide chapter 21.

## [0.18.0] — 2026-11-13 (not yet tagged) — Phase 17: grants and finance

### Added

- Planning tools (toolset `planning`):
  - `search_grant_opportunities` (PLAN-06): open funding opportunities for a health agency. Federal
    listings come from the Simpler.Grants.gov search API when the nurse sets the optional
    `PHN_GRANTS_GOV_API_KEY`; without a key, or when the request fails, the answer says
    `mode: snapshot` and why. The bundled snapshot has no federal listings: every federal grant
    site tried was blocked from the build environment, so the live path was tested only against a
    local server answering in the specification's shape. Massachusetts listings are the Operational
    Services Division's weekly "New Bids Available" list of COMMBUYS bids, read live from mass.gov
    (PDF, cached six hours; a changed layout is rejected, not guessed at) or from the bundled
    `commbuys-weekly-bids` snapshot (147 bids, list dated 2026-11-09). Filters: topic, eligibility,
    closing-date window, award size, grants-only; filters that cannot apply to a COMMBUYS bid are
    listed as not applied. It also returns where else to look and Massachusetts procurement facts
    from DPH and OSD pages. Only topic words and filters are sent out.
  - `analyze_funding_announcement` (PLAN-07): reads a NOFO or Massachusetts RFR saved in `/data`
    (PDF, DOCX, HTML, TXT, Markdown) into a compliance checklist by pattern matching: eligibility,
    dated deadlines with days from today, page, word, and character limits, format rules, required
    attachments and SF form numbers, scoring criteria with points, dollar amounts, the project
    period, the submission method, and a response outline. It always lists what it could not find.
    Procurement contacts are never returned. Writes a DOCX and an XLSX checklist to `/data/output`.
    Tested on synthetic notices and on two public Massachusetts announcements, not on a federal
    NOFO.
  - `draft_grant_narrative` (PLAN-08): template plus local fill (ADR 0008) for a needs statement,
    project narrative, and sustainability section, organized by the scoring criteria. Statistics
    appear only from the `data_points` the nurse passes, each with geography, source, and year;
    program facts appear only from `facts`; everything else is a `[NURSE: ...]` placeholder, and a
    test checks that the draft contains no number that was not in the input. Writes a DOCX.
  - `build_program_budget` (PLAN-09): a line-item program budget in federal object classes (labeled
    in the order of SF-424A Section B) with up to five years, yearly escalation, cost share and a
    required-match check, and indirect costs on the right base (the de minimis rate of up to 15
    percent of modified total direct costs under 2 CFR 200.414(f), or a negotiated rate; modified
    total direct costs follow 2 CFR 200.1). Lines are typed or read from a CSV/XLSX in `/data`
    through the local-data guardrails. Writes an XLSX workbook with live formulas (recalculated in
    LibreOffice and matched against an independent Python computation, 341 cells, 0 differences) and
    a DOCX budget justification with `[NURSE: ...]` placeholders. The response returns category
    totals, never individual salaries or lines; a category total for a one-person category equals
    that person's pay (a caveat says so).
  - `economic_evaluation_calculator` (PLAN-10): cost per participant, cost per outcome, the ICER
    against a comparator (dominant and dominated programs get no ratio), net benefit, benefit-cost
    ratio, return on investment, payback year, and break-even, over up to 30 years with discounting
    (default 3 percent), a one-way sensitivity analysis (tornado table) and an accessible tornado
    chart (SVG and PNG, with a text alternative). The plain-language reading is guarded against
    over-claiming. Golden tests reproduce the benefit-cost results in Table 2 of CDC's MMWR
    2024;73(31).
- Leadership tools (toolset `leadership`):
  - `analyze_health_department_budget` (LEAD-01): analyzes the department's budget or ledger file
    (CSV or XLSX in `/data`) and returns category totals only: year-to-date spending and revenue
    against the share of the fiscal year passed (July 1 start by default), money available after
    encumbrances, flags for lines over budget or ahead of or behind pace, the revenue mix (fees and
    revolving funds, municipal appropriation, state Public Health Excellence grants, other state,
    federal, other grants), cost per service from volumes and allocation shares the nurse supplies,
    budget per resident beside DPH's published 2022-2023 figures, and year-end forecasts by two
    labelled methods. Vendor, employee, and payee columns are never read; all salary lines are
    combined into one row that is hidden everywhere when fewer than three positions stand behind it.
    Writes an XLSX with live formulas and a DOCX summary.
  - `model_staffing_needs` (LEAD-02): estimates public health nurse FTE from annual volumes and
    minutes per unit the nurse supplies, with an adjustable productive-hours calculation (the
    defaults are labelled placeholders), scenarios, the gap to current FTE, and residents and
    clients per nurse. No staffing benchmark is bundled (none could be read from a fetchable
    source); it compares only with a benchmark the nurse gives. Writes an XLSX with live formulas
    and a DOCX summary.
- MCP prompt `prepare_grant_application`: search opportunities, read the announcement, plan the
  project, build the budget, gather community data, draft the narrative, and check the result.
- Content drafted from eCFR, CDC, DPH, OSD, and DLS sources, all pending review:
  `content/planning/grant_sources.yaml`, `announcement_patterns.yaml`, `narrative_templates.yaml`,
  `budget_reference.yaml`, and `economic_evaluation.yaml`, and `content/leadership/department_finance.yaml`
  and `staffing_model.yaml`, with schemas.
- Snapshot `commbuys-weekly-bids` (`xtask/scripts/build_grants_snapshots.py`).
- Synthetic fixtures with seeded identifiers (a synthetic NOFO and RFR, budget lines, program budget and
  ledger files, staffing volumes); every new data tool is in the leak test.
- Acceptance script `docs/acceptance/phase-17.md` and user guide chapter 20.

### Changed

- `phn-data`: `HttpClient::post_json_bytes` for JSON POST requests (used by the grants search).
- `analyze_health_department_budget` accepts a ledger column whose only privacy finding is an "account" header (`ledger_column`); see 1.0.0.

## [0.17.0] — 2026-11-13 (not yet tagged) — Phase 16: program planning, evaluation, and evidence

### Added

- Planning tools (toolset `planning`):
  - `build_logic_model` (PLAN-01): a program logic model from the nurse's description (inputs,
    activities, outputs, short-term, intermediate, and long-term outcomes, assumptions, and external
    factors), organized as CDC's Program Evaluation Framework (2024, Step 2) describes. Checks
    structure and wording, reads the model as if-then statements, drafts CDC's six-part narrative,
    and draws it as Mermaid text and a self-contained accessible SVG with alt text and a full text
    description. Writes a one-page Word (or PDF) table, the SVG, a PNG, and the Mermaid text to
    `/data/output`.
  - `write_smartie_objectives` (PLAN-02): objectives written and checked against CDC's seven SMARTIE
    questions: flags a missing baseline, target, unit, date, population, data source, or equity
    dimension, a target that moves the wrong way, and unreadable dates, and computes the change from
    baseline to target. Healthy People 2030 lookup by topic, keyword, or objective ID (national
    baseline, target, status, and Leading Health Indicator flag), read live from ODPHP or from the
    bundled snapshot `healthy-people-2030-objectives` (507 objectives).
  - `build_evaluation_plan` (PLAN-05): an evaluation plan on the six steps of CDC's Program
    Evaluation Framework (2024): purpose, users, uses, evaluation types chosen from the program's
    stage, an evaluation matrix drafted from a logic model or the nurse's own questions (CDC's MMWR
    Table 5 example questions used word for word; every tool-written cell is marked as a
    suggestion), an analysis plan, a use and dissemination plan, a timeline, and an optional RE-AIM
    lens, as a Word (or PDF) plan with blanks.
  - `find_evidence_based_interventions` (PLAN-03): Community Guide evidence for a health topic from
    the bundled snapshot `cpstf-publications` (145 journal papers PubMed attributes to the
    Community Preventive Services Task Force, 2000 to 2026), with an optional live PubMed search for
    newer papers. A rating (strong, sufficient, or insufficient evidence; recommended or not) is shown
    only when the abstract's own words give it. The Community Guide and What Works for Health sites
    were blocked from the build network, so What Works for Health has no data and the result says
    what to check on each site.
  - `apply_behavior_change_theory` (PLAN-04): maps a program's problem, population, setting,
    barriers, and planned components onto 48 constructs of eight theories and frameworks (Health
    Belief Model, Social Cognitive Theory, Theory of Planned Behavior, Transtheoretical Model,
    Socio-Ecological Model, Diffusion of Innovations, COM-B with the Behaviour Change Wheel, and
    Self-Determination Theory), with strategy and measure ideas. Constructs are paraphrased from
    nine open-access papers and CDC's social-ecological model page; 24 of the 62 strategies come
    from a published table and the rest are marked as this project's suggestions. Text with
    identifiers is refused.
  - `track_program_performance` (PLAN-11): reads a program's results by period from a local
    CSV/XLSX and compares them with the nurse's targets: cumulative totals and pooled percentages,
    progress, pace against the program year, and run-chart or p-chart trend flags. Only aggregates
    by period are returned; small counts are hidden and the totals that would reveal them are
    withheld. Writes a dashboard report, a workbook with live formulas, and a chart per measure to
    `/data/output`.
- Evidence-based practice tools (toolset `ebp`):
  - `appraise_evidence` (EBP-03): guided critical appraisal of a study the nurse describes or
    supplies. It suggests the design from words in the text, then lists that design's questions with
    their sources (Cochrane 2011 risk of bias and CONSORT 2010, ROBINS-I, STROBE-based prompts,
    AMSTAR 2, a qualitative appraisal guide, SQUIRE 2.0). The tool never answers a question: it
    records the nurse's answers, lists the ones to look at again, and applies each instrument's own
    overall rule where one exists. It adds the nurse's GRADE or GRADE-CERQual judgments, an optional
    Crossref DOI lookup (only the DOI is sent), and a Word record. CASP is linked with its license
    terms flagged, not copied. Files with identifiers are refused and the text is never returned.
  - `build_evidence_synthesis_table` (EBP-04): a synthesis table from the studies the nurse appraised
    as Word and Excel (live summary formulas and a PRISMA counts sheet), and a PRISMA 2020 flow
    diagram (Mermaid, accessible SVG, text) from the counts the nurse enters, after checking the
    arithmetic.
  - `plan_quality_improvement` (EBP-05): guided quality improvement planning with the Model for
    Improvement: aim statement, outcome, process, and balancing measures, a driver diagram, a
    fishbone diagram, a process map (Mermaid, accessible SVG and PNG, text outline), PDSA cycle plans,
    and the SQUIRE 2.0 checklist (paraphrased, with a license note). From the nurse's measure data it
    draws a run chart with the IHI/Perla and Anhøj rules, or an XmR, p, u, or c control chart through
    the existing `phn-epi` SPC engine, with small counts hidden. Writes a DOCX plan and an XLSX tracker
    with live formulas (recalculated in LibreOffice). Golden tests compare the run chart rules with an
    independent Python computation and the Anhøj limits with the published table.
- Healthy People 2030 (`phn-data::healthy_people`), CPSTF, and Crossref (`phn-data::crossref`,
  one-week cache, response validated before caching) adapters. The Crossref adapter was never
  reached live (the host was blocked from the build network).
- Content drafted from CDC, ODPHP, and open-access journal articles, all pending review:
  `content/planning/logic_model.yaml`, `smartie.yaml`, `evaluation_framework.yaml`,
  `behavior_theories.yaml`, and `evidence_sources.yaml`, and `content/ebp/appraisal_checklists.yaml`,
  `grade_certainty.yaml`, `evidence_synthesis.yaml`, and `quality_improvement.yaml`, with schemas.
- Snapshots, each with a builder in `xtask/scripts/` that has a `--check` mode:
  `healthy-people-2030-objectives` and `cpstf-publications`.
- Synthetic fixtures with seeded identifiers (program metrics, program outcomes, a study abstract, a QI
  clinic measure file); every new data tool is in the leak test.
- Acceptance script `docs/acceptance/phase-16.md` and user guide chapter 19.

### Changed

- `ebp/spc.rs` (EBP-06) exposes `Prepared` and `prepare_file` as `pub(crate)` so the QI tool can reuse
  its event-file grouping and suppression; no behavior change.

## [0.16.0] — 2026-11-13 (not yet tagged) — Phase 15: chronic disease prevention and community assessment

### Added

- Chronic disease tools (toolset `chronic`):
  - `community_bp_screening_toolkit` (CHR-02): a community blood pressure screening kit for adults:
    CDC's measurement technique checklist (two steps that are standard practice but not found at a
    source are marked "not verified"), the current ACC/AHA categories (the 2025 AHA/ACC guideline
    retired and replaced the 2017 one; each category carries its source and confidence), the
    severe-range wording and CDC's pregnancy thresholds, referral tiers labelled "tool default,
    pending review", a screening log template (XLSX and CSV), three follow-up letter templates, a
    reference card, and a plain-language client handout, written to `/data/output`. It classifies
    typed readings (averaged; any severe reading flagged). A local screening log is summarized only
    as suppressed one-way counts, and follow-up letters filled from its rows go to `/data/output`
    only.
  - `diabetes_prevention_finder` (CHR-03): scores the CDC Prediabetes Risk Test from CDC's published
    points, checks CDC's eligibility rules for the National Diabetes Prevention Program lifestyle
    change program, and lists the Massachusetts sites on DPH's "Active MA DPPs" list for a town, county,
    or the state from a bundled snapshot (`ma-dpp-programs`, 73 sites, retrieved 2026-11-12). It explains
    CDC's delivery modes and the Medicare Diabetes Prevention Program facts from CMS, writes an optional
    plain-language handout, and summarizes a screening-event file as suppressed counts. CDC's registry
    could not be read, so it gives no recognition status, delivery mode, or online providers, and it
    makes no MassHealth statement.
  - `preventive_services_lookup` (CHR-04): the USPSTF grade A and B recommendations that fit a person's
    age, sex at birth, pregnancy status, and risk factors, each with its grade, statement date, the
    Task Force's own wording, what the tool could not check ("confirm"), a plain-language client summary,
    and related Massachusetts programs (the Massachusetts Breast and Cervical Cancer Program, the MA
    Colorectal Cancer Control Program, and 1-800-QUIT-NOW, from DPH pages). A recommendation that depends
    on a risk factor that was not given goes under "needs review", never dropped. When online it adds
    ODPHP's MyHealthfinder list. Bundled snapshot `uspstf-ab-recommendations` (53 recommendations from 46
    statements, 2016 on), built from the published statements read as PubMed abstracts (the USPSTF site
    and API were blocked). Optional handout to `/data/output`.
  - `asthma_home_assessment` (CHR-05): the CDC, EPA, and HUD home asthma trigger checklist for home
    visitors, pointing at the 105 CMR 410 sections already in the Phase 14 housing content; DPH's asthma
    control table (adapted from NHLBI EPR-3, screening level) with a classifier; the NHLBI asthma action
    plan template with peak flow zones; and DPH's Massachusetts Asthma Home Visiting Program Directory.
    The Asthma Control Test is licensed and is not reproduced. Writes the blank checklist and action plan
    to `/data/output`; fills action plans for the people in a local CSV/XLSX into `/data/output` only.
- Community health assessment tools (toolset `equity`):
  - `community_health_assessment_builder` (EQ-08): guides a MAPP 2.0 community health assessment for a
    town or county: the three phases and three assessments (verified against two 2025 journal articles;
    NACCHO's own pages were unreachable, and the result says so), the community status assessment
    assembled by calling the EQ-01 profile, EQ-02, EQ-03, and the Phase 14 environmental justice, climate,
    food access, drinking water, and fluoridation tools (a section that fails is reported with its reason),
    a 13-sector partner checklist (a partner roster is counted by sector), nearby hospitals from a new CMS
    snapshot (84 Massachusetts hospitals), the federal CHNA rule (26 CFR 1.501(r)-3) and the Attorney
    General's Community Benefits Guidelines, and the priority lists found in CHNA files the nurse saves in
    `/data`. Writes draft report sections to DOCX or PDF.
  - `prioritize_health_issues` (EQ-09): ranks community health issues with the Hanlon method
    (BPR = ((A + B) x C / 3) x D, with the PEARL test as a multiplier, a pre-screen, or off), a strategy
    grid, and nominal group scoring from vote totals. Ties share a rank, missing ratings are listed as
    still needed (never zero), and the method and rationale are written to DOCX or PDF.
  - `analyze_qualitative_data` (EQ-10): reads focus group and key informant transcripts locally, removes
    names and other identifiers, codes them with the nurse's codebook or a draft starter codebook, and
    returns counts only: a scrub report, the codebook, and for each theme the number of different
    participants who raised it. A code raised by fewer than three participants is left out entirely. The
    de-identified coded excerpt matrix (XLSX) and a theme summary (DOCX) go to `/data/output`.
- Epidemiology tool (toolset `epi`):
  - `geocode_and_aggregate_addresses` (EPI-13): locates the addresses in a file in `/data` offline, against
    MassGIS Master Address Data points and Census TIGER address ranges, and returns only counts by town,
    census tract, or block group, with small counts suppressed. It reports how many addresses were located
    at each precision (exact point, interpolated, street only, ZIP code only, town only, not located) and
    never returns an address or coordinate. The full street index is optional (35.5 MB for 351 towns,
    built by `xtask/scripts/build_geocoder_index.py --full` and installed in `/data/geocoder/` or
    `/cache/geocoder/`); a four-town sample (Alford, Amherst, Northampton, Pelham) and a statewide ZIP-code
    and place-name to town crosswalk are bundled, so without the index the tool still counts by town.
    Design and index format: ADR 0011.
- MCP prompt `community_health_assessment`: a MAPP 2.0 workflow that chains EQ-01 to EQ-10 and the Hanlon
  calculator, names the toolsets it uses (`equity`, `chronic`, `env`), and keeps transcripts and client
  files in the data folder.
- Content drafted from CDC, CMS, DPH, EPA, HUD, NHLBI, and open-access journal sources, all pending review:
  `content/chronic/bp_screening.yaml`, `diabetes_prevention.yaml`, `preventive_services.yaml`, and
  `asthma_home.yaml`, and `content/equity/cha_framework.yaml`, `prioritization.yaml`, and
  `qualitative_codebook.yaml`, with schemas.
- Snapshots, each with a builder in `xtask/scripts/` that has a `--check` mode: `ma-dpp-programs`,
  `uspstf-ab-recommendations`, `cms-ma-hospitals`, `ma-geocoder-sample`, and `ma-geocoder-crosswalk`.
- Synthetic fixtures with seeded identifiers (a blood pressure screening log, prediabetes responses, asthma
  plans, focus group and interview transcripts and a names file, an address list); every new data tool is
  in the leak test.
- Acceptance script `docs/acceptance/phase-15.md` and user guide chapter 18.

## [0.15.0] — 2026-11-11 (not yet tagged) — Phase 14: environmental health and climate

### Added

- Environmental health tools (toolset `env`):
  - `drinking_water_quality_lookup` (ENV-03): the public water systems for a town from EPA
    SDWIS (live through Envirofacts, with a bundled snapshot): health-based violations, other
    violations by category, the latest lead and copper 90th percentiles against the action
    levels, MassDEP PFAS6 results for treated water (EEA Data Portal), what residents can do,
    the Massachusetts and federal PFAS and lead standards, and private well testing guidance
    when community systems do not serve most of the town.
  - `environmental_justice_screen` (ENV-04): Massachusetts 2020 environmental justice block
    groups for a town, county, or the state, by criterion and combination, with the people
    living in them (EEA's municipal statistics for towns), English-isolation block groups and
    the largest limited-English language groups, SVI context, what designation changes (MEPA,
    translation and interpretation, EEA policy, DPH's vulnerable health criteria), an optional
    map and table, and an optional summary of an archived EJScreen 2.3 file saved in `/data`
    (averages and counts only).
  - `climate_health_vulnerability_profile` (ENV-05): a town profile for MVP and hazard
    mitigation planning: extreme heat days, tree canopy, and impervious surface (CDC/ATSDR Heat
    and Health Index), NFIP flood claims and Community Rating System status, social
    vulnerability and climate-sensitive health, MVP region and plan status, FEMA hazard
    mitigation plan status (live from OpenFEMA, with a snapshot), and the most important
    vulnerabilities by labeled tool-default rules, with a Word option.
  - `vector_borne_disease_risk` (ENV-06): DPH's EEE and West Nile virus risk levels by town
    (live in season; otherwise the dated last levels, clearly not current risk) with DPH's
    response for each level, positive mosquito and animal findings, tick-borne disease cases
    and rates by county (2022–2025) and emergency department visits, and prevention messages
    for the public, schools, camps, and outdoor workers, with an optional one-page notice.
  - `housing_conditions_health_guide` (ENV-07): the State Sanitary Code (105 CMR 410.000) for
    moisture and mold, pests, heat, hot water and utilities, lead, smoke and CO alarms, and
    trash: code sections, owner and occupant duties, conditions always deemed to endanger
    health, health effects, the inspection process and timelines, tenant protections (with a
    not-legal-advice notice), tenant teaching sheets at or below grade 8 (agency translations
    only), and inspection and repair request letters filled locally into `/data/output`.
  - `chemical_exposure_reference` (ENV-08): the NIOSH Pocket Guide entry and current IDLH,
    ATSDR minimal risk levels, and ToxFAQs links for a chemical by name, synonym, or CAS number
    (ambiguous names are refused with the candidates), with notes and contacts for spills,
    indoor air, contaminated sites, drinking water, and workplaces. Every answer starts by
    saying that occupational limits are not community limits and that MRLs are screening
    levels.
- Chronic disease tools (toolset `chronic`):
  - `food_access_mapper` (CHR-06): USDA Food Access Research Atlas low-income, low-access
    tracts under all four definitions (the 2025 map, which counts any SNAP store, and the 2019
    supermarket map carried to 2020 tracts) for a town, county, or the state, with the people
    affected, vehicle and SNAP context, HIP and SNAP facts, counts by town from your pantry,
    SNAP retailer, and HIP vendor files, a tract map, a Word resource list, and an Excel
    workbook. Site names and addresses go only into the output files.
  - `community_water_fluoridation_status` (CHR-07): a town's fluoridation status from DPH's
    2023 list and CDC's 2025 My Water's Fluoride report with the systems behind it, a statewide
    summary, the standards, each body's current position and the recent evidence reviews side
    by side with their dates, M.G.L. c. 111, § 8C in plain language, and an optional board of
    health brief.
- Content drafted from EPA, MassDEP, DPH, EEA, MEPA, CDC, ATSDR, NIOSH, FEMA, USDA, and DTA
  sources and the statutes and regulations they cite, all pending review:
  `content/env/drinking_water.yaml`, `environmental_justice.yaml`, `climate_vulnerability.yaml`,
  `vector_borne.yaml`, `housing_conditions.yaml`, `chemical_exposure.yaml`, and
  `content/chronic/fluoridation.yaml` and `food_access.yaml`, with schemas.
- Snapshots, each with a builder in `xtask/scripts/` that has a `--check` mode: EPA SDWIS and
  MassDEP PFAS6, CDC My Water's Fluoride and DPH's fluoridation list, the 2020 EJ block groups
  and town statistics, the Heat and Health Index, NFIP claims and community status, FEMA hazard
  mitigation plans, MVP communities, DPH arbovirus risk levels and positives, DPH tick-borne
  disease cases and ED visits, the NIOSH Pocket Guide and IDLH table, ATSDR MRLs and ToxFAQs,
  and the USDA Food Access Research Atlas.
- Synthetic fixtures with seeded identifiers (an EJScreen extract, housing requests, a pantry
  list); every new data tool is in the leak test.
- Acceptance script `docs/acceptance/phase-14.md`; user guide chapter 17 and a rebuilt guide
  for 110 tools (135 pages).

### Fixed

- `heat_health_action_planner` (and the new climate profile): the population table's "about how
  many people" multiplied every percent by the total population, so a town could show more adults 65+ with a disability than adults 65+. Each count
  now uses the indicator's own base (adults 65+, people age 5+, or households), and tables show
  the unit when it is not people.

## [0.14.1] — 2026-11-11 (not yet tagged) — Peer review of Phases 0–13

A review of the code and documentation for Phases 0–13 found errors in calculations, places where
private information could reach the AI model, and documentation that no longer matched the code.
This release fixes them. No tools were added or removed; `docs/tools.md` describes each tool's
new behavior. ADR 0010 records the decisions made in the review.

### Security and privacy

- Local files: headings and titles are now redacted like the rest of the text in `read_local_file`
  and `search_agency_policies`, and documents that hold a client table or roster (in Word tables
  or tab-, pipe-, or semicolon-separated text) are redacted strictly.
- Local files: column headers and sheet names that look like names or values (a file with no
  header row) are shown as `column_N` and `sheet_N`; more name and ID columns are recognized
  ("First", "Last", "Student", "Guardian", "ID", "Case #"); more date, phone, SSN, and record
  number formats are detected.
- `read_local_file` sample rows: ZIP codes use the Safe Harbor rule (000 for low-population
  prefixes), a birth year that shows an age of 90 or more is `90+`, and long numbers in unknown
  columns are masked.
- `profile_local_dataset`: no statistics for a column with fewer values than the suppression
  threshold; ages over 89 top-coded before any statistic; smallest and largest values hidden for
  ID-like columns.
- `apply_small_cell_suppression`: numeric label columns (grade, age, week, ZIP) are no longer
  treated as counts, which could undo the suppression; label cells with names, phone numbers, or
  free text are refused; error messages no longer quote cell values.
- `deidentify_dataset`: Safe Harbor removes dates and ZIP codes inside text cells; a limited
  data set drops latitude and longitude.
- `check_document_accessibility`: quoted excerpts are redacted, and the privacy block reports it.
  `export_document` mail merges no longer return merged text in the accessibility messages.
- Small counts that could be worked out by subtraction are now hidden in
  `track_school_screenings`, `build_school_health_annual_report`, `screen_health_related_social_needs`
  (by group), `score_mental_health_screen` (file mode), `plan_casper_assessment`, and
  `create_spc_chart` (center line and limits withheld); `analyze_outbreak_line_list` withholds the
  median onset and exposure window in very small outbreaks; `create_chart` hides a suppressed
  value's confidence limits.
- Language cells are never returned: unrecognized languages are reported as "Other" in school
  letters and health plans, and letter files are never overwritten.
- Staff contacts (ADR 0010): a name not found on the roster gets no contact details, instead of
  those of the first person in the same position (decided by the product owner on 2026-11-11).
  Typed-in phone numbers and emails are now detected in every field of the Phase 13 tools.
- A saved suppression threshold outside 2–30 (for example after a hand edit) is ignored instead of
  switching suppression off.
- CSV output marks cells that start like a spreadsheet formula as text.
- Network: requests send no Referer header; openFDA's API key is no longer stored in the cache;
  the NWS forecast link is followed only to api.weather.gov; an error page from a data source is
  no longer cached as data.
- Release workflow: the image is scanned before anything is pushed, the SBOM lists the Rust
  crates (built from the source tree), and the workflow runs only for version tags.

### Fixed

- Statistics: rank tests (Mann–Whitney, Wilcoxon) gave p = 0 when there was no difference at all;
  they now give p = 1. t tests and ANOVA on data with no variation give p = 1 instead of an
  undefined value.
- Statistics: chi-square degrees of freedom and Cramér's V ignore empty rows and columns;
  Breslow–Day degrees of freedom ignore empty strata.
- Statistics: logistic regression in `run_statistical_test` warns about separation and models that
  did not converge, instead of reporting huge odds ratios as "not significant".
- Statistics: SPC trend and shift rules no longer count repeated values or points on the center
  line (a flat series is never a trend).
- Statistics: an all-zero count series is refused for rate and trend models; a correlation with a
  constant variable is refused; Fisher's exact test falls back to chi-square when it cannot be
  calculated; very large 2×2 tables skip the exact methods instead of running for minutes.
- `analyze_trends`: seasonal adjustment is on by default only with at least two years of monthly
  or weekly data; the seasonal peak is placed correctly for labels such as "Mar 2024".
- `age_adjust_rates`: warns when age groups do not cover all ages or differ between groups.
- Dates (every tool that reads dates from files, and the school, immunization, growth, and
  communicable disease tools): "3/5/26" was read as the year 26; two-digit years are now read in
  the current century unless that would be more than a year in the future (then the previous
  century), and years outside 1900–2100 are refused instead of crashing date arithmetic.
- Disease and pathogen lookups no longer guess: `lookup_reportable_condition`,
  `calculate_isolation_and_exclusion`, and `infection_control_for_congregate_settings` had
  answered "hepatitis" as hepatitis A, "gastroenteritis" as group A strep, "stomach flu" as
  influenza, and "ebola" as MRSA. Ambiguous or general queries now list the candidates.
- `plan_contact_tracing`: workbook columns now say which contacts each exclusion applies to
  (pertussis and hepatitis A rules had read as applying to every contact).
- `assess_measles_exposure`: an infant under 12 months with reported doses was treated as immune;
  PEP deadlines now count only exposure inside the case's infectious period.
- `calculate_isolation_and_exclusion`: "still excluded on" checks work for rules counted in hours.
- `plan_tb_contact_investigation`: a symptom onset date now sets the infectious period even when
  "symptomatic" was not marked; ages such as "8 months" are read.
- `analyze_outbreak_line_list`: an unknown agent is a caveat, not an error; exposures that could
  not be compared are listed; a "Meal Time" column is no longer taken for the onset time.
- `get_notifiable_disease_trends`: the "last 4 weeks" baseline was 3 weeks long for week 53.
- `check_ma_school_immunization_requirements`: students 7 and older with DTaP gaps were told
  "not met" and offered DTaP (now "needs review"); doses too soon after an invalid dose, numbered
  roster columns ("MMR 1"), and Exemptions cells such as "Religious" are handled.
- `forecast_immunizations_due` (file mode): doses dated before birth or after the assessment date
  are counted and listed instead of being silently treated as evaluated.
- `vaccine_temperature_excursion_helper`: Fahrenheit readings such as 46.3 °F were judged in range.
- `lookup_medication_information`: shortages are searched by generic name (a brand name had
  returned "none listed"); a close but inexact RxNorm match now carries a caveat.
- `score_mental_health_screen`: a C-SSRS screener with unanswered deciding items was scored low
  risk (now "risk level not yet determined"); unanswered suicide-risk items are flagged.
- `find_substance_use_treatment`: "Medicaid" and "Spanish" filters matched the wrong SAMHSA values;
  out-of-state facilities are flagged.
- `nursing_home_quality_lookup` and `find_community_resources`: facilities listed under a village
  or neighborhood (Hyannis, Dorchester, Middleboro) were missed; they are matched by ZIP code.
- `elder_protective_services_guide`: whether a role is a mandated reporter now follows the list for
  the chosen pathway (c.19A or c.19C).
- `steadi_fall_risk_assessment`: a count of falls now counts as a fall in the past year.
- School tools: "Negative" results count as passed; heights such as 4'10" are no longer read as
  410; "Speech/Language Services" is no longer taken for the language column; a student with two
  conditions gets both health plans; a "yes" notice without a date is not a gap; the annual report
  in July or August covers the school year that just ended; visit reasons no longer match inside
  words ("distress" is not "stress").
- `check_developmental_milestones`: "not yet" on the corrected-age checklist now counts.
- `get_active_hazard_alerts` and `heat_health_action_planner`: a stale cached copy of the NWS feed
  was shown as current alerts, and a failed feed read as "no alerts"; ended alerts are dropped.
- `heat_health_action_planner`: a HeatRisk image from an earlier forecast is no longer paired with
  new dates; the peak day is the first day of the highest level.
- `air_quality_protective_guidance`: concentrations above the 8-hour ozone and 1-hour SO2 tables
  are refused instead of extrapolated.
- `build_community_health_profile`: a top-coded median income showed as $250,001.
- `translate_health_material`: numbers such as 3.5, 1,000, and 9:30 were reported as missing;
  glossary terms match whole words and plurals.
- `check_readability`: syllable counting fixed for many common words, such as safety, serious,
  diarrhea, and influenza (grades can shift a little); `draft_press_materials` no longer flags
  "Hispanic" as containing "panic".
- `omaha_system_coder`: negated problems ("no income") are listed for review instead of dropped.
- `plan_casper_assessment`: the sampling interval accounts for blocks selected twice; analyzing
  returned data without a seed no longer draws a new sample.
- `generate_ics_forms`: the ICS 207 shows branches; an incident name with a slash no longer
  changes the output folder.
- `continuity_of_operations_planner`: staff at the peak were undercounted (10 staff at 80% absence
  gave 1); a position held by several people no longer makes someone their own successor.
- `design_hseep_exercise`: warns when the schedule is longer than the exercise; MSEL events are
  numbered within each module.
- `build_after_action_report`: ratings naming "PHEP 13: …" were dropped silently; the objectives
  from `design_hseep_exercise` can be passed unchanged.
- `crisis_risk_communication_builder`: an agency translation was used even after the English
  message was changed; "space heater fire" no longer brings up the heat messages; bracketed
  placeholders no longer pass the checks.
- `export_document`: a `---` line no longer starts a new page; nested lists and indented text are
  kept in order; words no longer run together in PDFs; repeated column names and long sheet
  names no longer break XLSX files; text such as "1,2" stays text.
- `create_chart` refuses a log scale for bar charts; charts with a very narrow range no longer hang.
- Crashes fixed for file names, sheet names, and dates with accented or very long text.
- `search_pubmed`: an NCBI error is reported instead of being cached as "no results".

### Changed

- The Docker image runs on Eastern time (`TZ=America/New_York`), so "today" and document dates no
  longer roll over at 7 or 8 p.m. (ADR 0010). `phn-mcp-server check` prints the local time.
- The Docker image makes `/cache` writable for Linux users who run it with `--user`. A
  `phn-cache` volume created by an earlier version must be removed once (see the README).
- `PHN_OFFLINE` accepts only 1/true/yes/on or 0/false/no/off, and `PHN_TOOLSETS` must name at
  least one toolset; anything else stops the server with an error.
- When `PHN_PROFILE` or `PHN_TOOLSETS` is set in the client configuration, `configure_jurisdiction`
  says that saved toolsets have no effect, and a disabled tool's error says to edit the
  configuration.
- `functional_needs_shelter_intake` uses the same translation folder names as the LTBI calendar
  and newcomer checklist (`haitian_creole`), and is marked as using the internet.
- The lead and newborn teaching sheets also accept a `haitian-creole` folder.
- The `investigate_outbreak` and `plan_tabletop_exercise` prompts name the toolsets they use and
  say what to do when one is off; the outbreak prompt no longer suggests "GI illness, agent
  unknown" as a disease name.
- MCP resources: content descriptions say "pending review" instead of "Reviewed reference
  content"; JSON and text snapshots have the right MIME type.
- Cached public data older than 90 days is deleted; a damaged cache is skipped instead of
  stopping a lookup; retrieval dates are local dates.
- Documentation: README (offline image install, repository layout, configuration), CONTRIBUTING,
  `project_plan.md` §5 (the architecture as built), the user guide's profile table, the acceptance
  scripts, and new ADR 0010 (superseding notes for ADRs 0001, 0004, and 0008; time zone; roster
  name matching).

## [0.14.0] — 2026-11-09 (not tagged) — Phase 13: emergency preparedness II (ICS forms, COOP, HSEEP exercises, after-action reports, CERC)

### Added

- Preparedness document tools (toolset `epr`):
  - `generate_ics_forms` (EPR-01): ICS 201, 202, 203, 204, 205, 205A, 206, 207, 208, 213, 213RR,
    214, and 215A in Word and/or PDF, with FEMA's titles, block numbers, and labels (checked
    against FEMA's v3/v3.1 forms), an assembled Incident Action Plan, blank forms with FEMA's block
    instructions, the required blocks still blank, and suggested objectives and safety lines for
    seven public health incident types.
  - `continuity_of_operations_planner` (EPR-07): a board of health continuity of operations plan
    by FEMA's ten continuity elements, with 19 Massachusetts essential functions (authority,
    priority tier, recovery time, minimum staff), orders of succession, delegations of authority,
    continuity facilities, communications, essential records, a 40% absenteeism staffing check,
    a first-year test, training, and exercise schedule, devolution, reconstitution, and a pandemic
    annex (Word plan and Excel workbook).
  - `design_hseep_exercise` (EPR-08): HSEEP exercise design for all seven exercise types, with a
    SMART check of each objective, mapping to the 15 CDC PHEP capabilities and FEMA core
    capabilities, fully developed tabletop scenarios for a measles outbreak, a coastal storm with
    a power outage, and extreme heat (Pilot 3), scenario seeds for six other hazards, and the
    Situation Manual, Facilitator Guide, Exercise Plan, MSEL, Exercise Evaluation Guides, feedback
    form, and sign-in sheet.
  - `build_after_action_report` (EPR-09): the HSEEP after-action report and improvement plan for
    an exercise or a real incident, with corrective-action, non-attribution, and client-identifier
    checks and participant feedback-form totals computed locally with small-cell suppression.
  - `crisis_risk_communication_builder` (EPR-10): CDC CERC message maps checked against the
    27-word, 9-second, 3-message rules, holding statements, FAQs, spokesperson preparation, and
    phase tasks for seven pre-scripted hazards or your own, with the town's priority languages,
    agency translations from `/data/templates/<language>/`, and translator briefs (no machine
    translation).
- MCP prompt `plan_tabletop_exercise`: an HSEEP tabletop exercise cycle (hazard context, design,
  player materials, hotwash, after-action report) using the Phase 12 and 13 tools.
- `epr::roster` and ADR 0009: staff names and positions may be passed in a request; phone
  numbers, emails, and radio call signs come only from a roster file in `/data` and are written
  only to documents in `/data/output`. Contact details found in a request are reported in the
  privacy block.
- Content drafted from FEMA's ICS forms, HSEEP 2020 and its templates, CDC's 2018 PHEP
  capability standards, the CERC manual and message-mapping guides, M.G.L. c. 111 and c. 114,
  and CDC and DPH hazard pages, all pending review: `content/epr/ics_forms.yaml`,
  `continuity.yaml`, `capabilities.yaml`, `hseep.yaml`, `exercise_scenarios.yaml`, and
  `cerc.yaml`, with schemas.
- Synthetic fixtures: a fictional staff roster and exercise feedback forms with seeded
  identifiers; every new tool is in the leak test.
- Acceptance script `docs/acceptance/phase-13.md`; user guide rebuilt for 102 tools (124 pages).

### Changed

- The tool registry lists phases as (phase, tools) pairs so a phase can have several
  work-package lists.
- COMM-02 exposes its language-priority and translator-brief helpers, and COMM-04 shares its
  loaded-word check, for reuse by the CERC builder.

## [0.13.0] — 2026-11-08 (not tagged) — Phases 7 to 12: school health, maternal and child health, PHN practice, healthy aging, substance use and communication, and emergency preparedness I

### Added

- User guide for public health nurses (`docs/user-guide/PHN-MCP-Server-User-Guide.docx`): an
  executive summary of the server's benefits, how it works and protects privacy, getting
  started, and for each implemented tool what it does, when to use it, how to ask for it, what
  it returns, and what to keep in mind; plus a tool index, glossary, and troubleshooting. The
  guide was first written after Phase 6, for 47 tools, and was rebuilt for this version's 97
  tools (116 pages). Built from `docs/user-guide/guide-content.js` by `build-user-guide.js`; a
  registry test keeps the guide's tool list in step with the implemented tools.

### Added — Phase 7: school health

- School health tools (toolset `school`): `get_ma_school_screening_requirements` (SCH-01),
  `track_school_screenings` (SCH-02), `school_medication_administration_guide` (SCH-03),
  `build_individualized_health_plan` (SCH-04), `concussion_return_to_learn_tracker` (SCH-05),
  `monitor_school_absenteeism` (SCH-06), `build_school_health_annual_report` (SCH-07), and
  `child_care_health_consultation_checklist` (SCH-08).
- Content drafted from 105 CMR 200.000, 201.000, 210.000 (12/5/2025), 300.000, 606 CMR 7.00,
  M.G.L. c. 71 sections 53-57, 54B, and 97, c. 111 section 222, DPH school health protocols and
  guidance, and CDC pages, all pending review: `content/school/*.yaml` with schemas, and the CDC
  BMI-for-age LMS reference copied from `bmiagerev.csv`.
- School-year (July-June) and grade logic, BMI-for-age percentiles (golden test against CDC's
  published percentiles), and letter and plan templates with local fill and agency translations
  in `/data/templates/<language>/`.
- Fictional fixtures (screening roster, absence log, health office visits, head injury log,
  health plan students) with a seeded generator, added to the leak test.

### Added — Phase 8: maternal, child, and family health

- `calculate_growth_percentiles` (MCH-03): WHO 2006 (birth to 24 months) and CDC 2000 (2 to 20
  years) percentiles and z-scores with CDC's 2022 extended BMI-for-age method, BMI categories,
  corrected age for prematurity, extreme-value flags, visit-to-visit change, and a batch mode
  that writes per-row results to the output folder. Snapshots `who-growth-standards` and
  `cdc-growth-charts` (`xtask/scripts/build_growth_charts.py`); content
  `content/mch/growth_interpretation.yaml` (pending review); golden tests against CDC's and
  WHO's published tables.
- `childhood_lead_screening_guide` (MCH-04): screening schedule and high-risk schedules from
  105 CMR 460.050, DPH's 2024 high-risk community list with each town's screening rate, venous
  confirmation and follow-up by blood lead level from DPH CLPPP, reporting, the Lead Law, family
  teaching, and an optional teaching sheet (English or an agency translation in
  `/data/templates/<language>/`). Content `content/mch/lead_screening.yaml` (pending review);
  snapshot `ma-lead-screening` from DPH's 2024 surveillance report
  (`xtask/scripts/build_lead_snapshot.py`).
- `mandated_reporter_51a_guide` (MCH-07): M.G.L. c. 119, s. 51A guidance (who, when, how,
  contents, penalties and protections, DCF definitions, domestic violence, DCF process, warning
  signs), the 48-hour written-report deadline, and the written-report narrative template; with
  `notes_path` it fills a draft from the nurse's local notes into `/data/output` and returns
  only file names and field counts. Content `content/mch/mandated_reporting.yaml` (pending
  review).
- `check_developmental_milestones` (MCH-02): CDC "Learn the Signs. Act Early." checklists
  (2 months to 5 years) with milestone IDs, act-early steps, AAP screening ages, the
  corrected-age checklist for preterm infants, Massachusetts Early Intervention referral, and
  the EI programs serving a town. Content `content/mch/developmental_milestones.yaml` (pending
  review); snapshot `ma-ei-programs` (DPH's EI program directory).
- `plan_newborn_postpartum_visit` (MCH-01): newborn and postpartum home visit plan (feeding and
  weight change, jaundice, AAP 2022 safe sleep, Massachusetts car seat law, newborn blood spot
  and hearing screening under 105 CMR 270.000 and M.G.L. c. 111, s. 67F, CDC Hear Her urgent
  maternal warning signs) with an optional DOCX visit plan and family teaching sheet. Content
  `content/mch/newborn_postpartum.yaml` (pending review; jaundice items low confidence).
- `estimate_benefit_eligibility` (EQ-06): income screen for WIC, SNAP, MassHealth, the Health
  Safety Net, and HEAP with the 2026 poverty guidelines and each program's current limits and
  effective dates. Content `content/equity/benefit_eligibility.yaml` (pending review; refresh
  annually).
- `map_lead_risk` (MCH-05): outreach ranking of towns or tracts (mean percentile rank of
  pre-1950/1980 housing, low income, young children, DPH screening gap and elevated blood lead
  prevalence) with DPH's high-risk designation and a choropleth map. Snapshot
  `ma-acs5-housing-age` (ACS B25034, `xtask/scripts/build_housing_age.py`).
- `maternal_infant_health_profile` (MCH-06): community births, low birthweight, preterm, and
  teen births with exact intervals and state/U.S. comparisons; statewide indicators and infant
  mortality by race and Hispanic ethnicity; PRAMS indicators; trends. Snapshot
  `ma-maternal-infant` (DPH birth, death, and PRAMS reports; CDC NCHS provisional estimates;
  `xtask/scripts/build_maternal_infant.py`).
### Added — Phase 9: PHN practice toolkit and case management

- `plan_tb_contact_investigation` (CD-11) and `ltbi_treatment_calendar` (CD-12): the 2005
  NTCA/CDC contact investigation rules (infectious period, Figure 2-4 priorities, Table 3 time
  frames, 8-10 week retest) with an XLSX tracking workbook and local contact-list summaries, and
  LTBI dose calendars, completion windows, and adherence checks for 3HP, 4R, 3HR, 6H, and 9H with
  an XLSX tracker and DOCX patient calendar (no dose calculation).
- `screen_health_related_social_needs` (EQ-05): CMS AHC HRSN scoring by domain with Massachusetts
  referral categories and a batch mode with suppressed counts.
- `omaha_system_coder` (CORE-01), `classify_intervention_wheel` (CORE-02), and
  `nursing_documentation_template` (CORE-03): Omaha System coding, Public Health Intervention
  Wheel classification with workload counts, and SOAP/DAR/narrative templates with a
  required-elements check; notes are analyzed locally and never returned.
- `find_community_resources` (EQ-07): the nurse's resource directory plus HRSA health center sites
  (new `phn_data::hrsa` adapter and `hrsa-health-centers-ma` snapshot with its builder script).
- `newcomer_health_navigator` (EQ-11): CDC domestic refugee screening, DPH/ORI programs, MassHealth
  coverage by immigration status including the October 2026 federal changes, and EA shelter
  resources.
- Content (pending review): `content/cd/tuberculosis.yaml`, `content/equity/ahc_hrsn.yaml`,
  `content/equity/newcomer_health.yaml`, `content/core/omaha_system.yaml`,
  `content/core/intervention_wheel.yaml`, `content/core/documentation_templates.yaml`; agency
  translations are read from `/data/templates/<language>/`; fictional TB contact, HRSN screening,
  and resource directory fixtures.
### Added — Phase 10: healthy aging and mental health screening

- `score_mental_health_screen` (MH-02): scoring engine for PHQ-2, PHQ-9, GAD-2, GAD-7, EPDS, the
  NIMH ASQ, and the C-SSRS screener with range checks, published cutoffs, safety-alert rules and
  cited steps, license terms per instrument, file mode with suppressed counts, and 50 golden cases.
- `find_crisis_resources` (MH-01): 988, the Behavioral Health Help Line, mobile crisis, the Crisis
  Text Line, and Community Behavioral Health Centers by town, with language access.
- `steadi_fall_risk_assessment` (AGE-01), `elder_protective_services_guide` (AGE-03),
  `find_ma_aging_services` (AGE-02), `explain_older_adult_coverage_options` (AGE-04),
  `nursing_home_quality_lookup` (AGE-06), `map_older_adult_vulnerability` (AGE-05), and
  `advance_care_planning_toolkit` (AGE-07).
- Town-to-service directory framework and snapshots (ASAPs, SHINE offices, senior nutrition
  projects, CBHCs) with `cargo xtask build-service-directories`; CMS Provider Data Catalog adapter
  (`phn_data::cms_provider`) with `cargo xtask build-nursing-home-snapshot`; older-adult ACS
  indicators (`phn_data::older_adults`) with `cargo xtask build-older-adult-snapshots`.
- Template + local fill helper (`phn_tools::aging::localfill`) with agency translations from
  `/data/templates/<language>/` and English fallback counted by language.
- Content (all pending review): `content/bh/mental_health_screening.yaml`,
  `content/bh/crisis_resources.yaml`, and `content/aging/*.yaml` (STEADI, protective services,
  aging services, coverage options, nursing home guide, advance care planning).
### Added — Phase 11: substance use, crisis resources, and health communication

- `check_readability` (COMM-01): reading-level formulas (Flesch Reading Ease, Flesch-Kincaid,
  SMOG with CDC's conversion table, Gunning Fog), plain-language flags with CDC Everyday Words
  alternatives, and the CDC Clear Communication Index scored from the nurse's answers. The text
  statistics moved to `phn_tools::comms::textstats` (VAX-09 now uses them) with better sentence
  splitting and syllable counting. Content: `content/comms/readability.yaml`. `comms.rs` became
  the `comms/` folder.
- `get_overdose_data` (MH-03): DPH town-level opioid-related overdose deaths (2015-2022 table,
  bundled), 5-year rates with exact intervals compared with the state, CDC/NCHS provisional county,
  state, and U.S. counts by drug class (live → cache → snapshot), and DPH's 2024 findings.
  Snapshots `ma-dph-opioid-deaths-town` and `cdc-vsrr-overdose-ma` (builders in `xtask/scripts/`);
  content `content/bh/overdose_data.yaml`.
- `get_youth_risk_behavior_data` (MH-07): DPH Youth Health Survey (2023, 2025) and Massachusetts
  YRBS (2023) report tables by sex, grade, race/ethnicity, and LGBTQ identity with 95% CIs and
  differences by non-overlapping intervals, and CDC YRBS mental health indicators for
  Massachusetts vs the U.S. Snapshots `ma-youth-health-survey` and `cdc-yrbs-mental-health-ma-us`;
  content `content/bh/youth_survey.yaml` (also holds DPH's 988 Lifeline text).
- `find_naloxone_and_harm_reduction` (MH-04): naloxone access (statewide standing order of July
  13, 2026, pharmacy policy, programs), syringe service program approval by town, fentanyl test
  strip facts, help lines, and a cited teaching script with an optional Word handout. Content
  `content/bh/substance_use_resources.yaml` (shared with MH-05).
- `find_substance_use_treatment` (MH-05): SAMHSA FindTreatment.gov adapter
  (`phn_data::findtreatment`, live → cache → bundled statewide snapshot `samhsa-findtreatment-ma`)
  with MOUD, setting, special-group, payment, and language filters, and the Massachusetts
  Substance Use Helpline.
- `plan_school_postvention` (MH-06): After a Suicide toolkit steps and talking points (with
  pages), message templates by cause-of-death status filled locally into Word files, and a
  safe-messaging check of drafts. Content `content/bh/postvention.yaml` and
  `content/comms/safe_messaging.yaml` (shared rules, `phn_tools::comms::safe_messaging`); fixture
  `fixtures/synthetic/data/postvention_details.csv`.
- `translate_health_material` (COMM-02): translation workflow with language priorities from EQ-03,
  agency translations in `/data/templates/<language>/`, a translator brief, glossary and
  back-translation checks, the qualified-translator draft label, and an XLSX review worksheet.
  Content `content/comms/translation.yaml`.
- `create_social_media_campaign` (COMM-03) and `draft_press_materials` (COMM-04): campaign
  calendars tied to observances with post checks and an XLSX export; press materials built and
  checked against CDC CERC media guidance with a Word export. Content
  `content/comms/campaigns.yaml` (platform limits and observance dates marked unverified).
### Added — Phase 12: emergency preparedness I: situational awareness and planning

- `get_active_hazard_alerts` (EPR-05): live NWS alerts for a town, county, or the state with
  cited health effects, at-risk groups, public health actions, and messages, and OpenFEMA
  disaster declarations (snapshot fallback).
- `model_dispensing_site_throughput` (EPR-02): M/M/c (Erlang C) model of POD and mass
  vaccination stations with sizing, waits, bottleneck, capacity, hours to reach a population,
  and an optional staffing workbook; `phn_epi::queueing` with golden tests.
- `air_quality_protective_guidance` (ENV-02): AirNow current and forecast AQI by reporting area
  with EPA and CDC protective actions, and 2024 Appendix G concentration-to-AQI conversion.
- `heat_health_action_planner` (ENV-01): NWS HeatRisk and forecast, heat-vulnerable population
  (ACS, PLACES, SVI, emPOWER), activation steps, and a DOCX heat response checklist.
- `hazard_vulnerability_assessment` (EPR-06): facilitated HVA seeded with ResilientMass 2023,
  FEMA declarations, and optional NRI tables, with an XLSX workbook.
- `functional_needs_shelter_intake` (EPR-03): CMIST intake template and blank form, local fill
  with agency translations, and shelter planning estimates.
- `get_electricity_dependent_population` (EPR-04): HHS emPOWER counts by town, county, and ZIP
  code from the emPOWER Map's JSON endpoints (snapshot fallback).
- `plan_casper_assessment` (EPR-11): CASPER cluster selection from 2020 census blocks with a
  reproducible seed, team materials, and weighted analysis (`phn_epi::survey`, golden test
  against samplics).
- Data adapters (`phn_data::nws`, `openfema`, `empower`, `airnow`, `heatrisk`,
  `census_blocks`, `projection`) and snapshots (FEMA declarations, emPOWER, 2020 census block
  housing units) built by `xtask/scripts/build_epr_snapshots.py`; eight content files in
  `content/epr/` and `content/env/` (pending review).
### Changed and fixed during integration

- The communication tools moved from `comms.rs` to `crates/phn-tools/src/comms/`; COMM-05 now
  lives in `comms/accessibility.rs`, and VAX-09's readability check uses the shared COMM-01 engine.
- Two user guide entries used an internet value the Word builder did not know, which broke
  `build-user-guide.js`; a registry test now checks every entry's value.
- The leak test also covers `create_social_media_campaign` pointed at an identifiable file.
- The data source registry lists every Phase 7–12 tool that reads local files or ACS data.
- `.gitignore` (a Python template) ignored every `env/` folder; exceptions keep
  `crates/phn-tools/src/env/` and `content/env/` in the repository.
- The former MOLST domain (molst-ma.org) no longer belongs to the program; the advance care
  planning tool does not link to it and marks its MOLST content unverified.
- The README documents the optional API keys (`PHN_CENSUS_API_KEY`, `PHN_NCBI_API_KEY`,
  `PHN_OPENFDA_API_KEY`, `PHN_AIRNOW_API_KEY`).

## [0.7.0] — 2026-11-08 (not tagged) — Phases 2 to 6: epidemiology engine, communicable disease, public data, immunizations, and immunization forecasting

### Added — Phase 2: epidemiology and biostatistics engine

- `age_adjust_rates` (EPI-02): direct age adjustment to the 2000 U.S. standard population using
  NCHS Master List weights, Fay–Feuer gamma intervals, and comparisons with a reference group.
- `analyze_two_by_two` (EPI-03): risk and odds ratios with Katz, Woolf, Fisher exact, and mid-p
  intervals; attributable fractions; chi-square and exact tests; Mantel–Haenszel and Breslow–Day.
- `calculate_sample_size` (EPI-04): surveys (finite population correction, design effect),
  two proportions (Fleiss with continuity correction), case-control, two means, power, clusters.
- `compute_standardized_ratio` (EPI-05): SIR/SMR with exact Poisson intervals and p-values.
- `analyze_trends` (EPI-09): APC, joinpoint-style segments and AAPC (BIC), interrupted time
  series, seasonality, Mann–Kendall, Durbin–Watson, and a trend chart.
- `run_statistical_test` (EPI-10): automatic test selection with assumption checks and
  sensitivity tests, on inline data or local files (aggregates only).
- `create_chart` (EPI-11): accessible SVG/PNG charts with a validated color-vision-safe palette,
  generated alt text, and a data table.
- `create_spc_chart` (EBP-06): run charts (Anhøj rules) and XmR, p, u, c control charts.
- `phn-epi` engine: GLMs by IRLS (Gaussian, Poisson, binomial, negative binomial), standardization,
  2×2 and stratified analysis, trend models, hypothesis tests (including Shapiro–Wilk), SPC, and
  sample size, with golden tests against R, SciPy, statsmodels, NCHS, and published tables. The
  R and Python scripts that produced each fixture are committed next to it.
- `phn-docs` chart renderer and accessible palette; `phn_data::socrata` client for data.cdc.gov.
### Added — Phase 3: communicable disease I

- `search_ma_disease_control_guide` (CD-02): offline tantivy search of a 1.1 MB plain-text corpus
  of 25 DPH guide chapters and 66 DPH fact sheet and guidance pages, built reproducibly by
  `cargo xtask build-guide-corpus` (URLs, file dates, and checksums in the snapshot manifest).
- `calculate_isolation_and_exclusion` (CD-03): a date-rule engine over a pending-review table of
  37 diseases (105 CMR 300.200, DPH guidance), with explicit day counting, settings, facts, and
  citations per rule.
- `analyze_outbreak_line_list` (CD-04): line-list column mapping, case definitions, suppressed
  person/place/time tables, exposure-specific attack rates, likely vehicles, exposure window.
- `generate_epi_curve` (CD-05): epidemic curves with incubation-based intervals, strata, exposure
  markers, and a described pattern; returned counts are suppressed.
- `plan_contact_tracing` (CD-06): infectious period, elicitation window, per-contact dates, and an
  XLSX contact-tracking template with date formulas.
- `assess_measles_exposure` (CD-07): evidence of immunity, PEP windows and IG doses, exclusion,
  priority groups.
- `detect_aberrations` (CD-10): EARS C1–C3, CUSUM, and Farrington-type (Noufaily) detection in
  `phn_epi::aberration`, golden-tested against R `surveillance`.
- `infection_control_for_congregate_settings` (CD-13): precautions, PPE, cohorting, and EPA
  disinfectant lists for congregate settings.
- The server's first MCP prompt, `investigate_outbreak`.
- Content (all pending review): `content/cd/disease_rules.yaml`, `measles_exposure.yaml`,
  `infection_control.yaml`; a fictional church-supper outbreak fixture for tests.
### Added — Phase 4: public data and evidence platform

- `build_community_health_profile` (EQ-01): ACS 5-year indicators (each cited by table ID, with
  margins of error and CV > 30% flagged), PLACES, and SVI for any Massachusetts town, county, or
  tract, compared with Massachusetts and the U.S.; works offline for all 351 towns.
- `get_social_vulnerability_index` (EQ-02): SVI 2022 tract ranks within Massachusetts or
  nationally, by theme.
- `language_access_profile` (EQ-03): LEP by language group (C16001, C16002, B16001) against the
  HHS safe harbor threshold, with margin-of-error-aware status. Content pending review; the
  safe harbor entry is low confidence.
- `measure_health_disparities` (EQ-04): rate ratios (exact conditional and log intervals), rate
  differences, index of disparity, between-group variance, excess events, and PAF; golden tests
  against R.
- `get_places_estimates` (CHR-01): CDC PLACES 2025 estimates with intervals and data years for
  counties, towns, places, tracts, and ZCTAs.
- `create_choropleth_map` (EPI-12): SVG/PNG choropleth maps with Fisher–Jenks or quantile
  classes, the accessible sequential palette, hatched suppressed areas, and alt text.
- `search_pubmed` (EBP-01) and `search_cdc_guidance` (EBP-02): NCBI E-utilities (no email,
  rate-limited, cached a day), the CDC content syndication API, and HAN notice pages.
- Data adapters (`phn_data::census`, `geography`, `boundaries`, `svi`, `places`, `eutils`,
  `cdc_guidance`, `apifetch`) with live → cache → snapshot fallback, and about 4 MB of new
  snapshots: ACS 2020–2024 indicators and detailed languages, SVI 2022, PLACES 2025, census
  geographies and crosswalks, and simplified town, tract, ZCTA, and school district boundaries,
  built reproducibly by `cargo xtask build-public-snapshots`.
### Added — Phase 5: immunizations I

- `get_immunization_schedule` (VAX-01) and `compare_vaccine_recommendations` (VAX-02): schedules
  and 17 recommendation topics with DPH, AAP, ACOG, AAFP, NEPHC, and CDC/ACIP positions, dates,
  and sources side by side; CDC minimum ages and intervals and contraindications.
- `check_ma_school_immunization_requirements` (VAX-04): a rule engine for the 2026–2027
  Massachusetts school requirements (child care/preschool, K–12, college) with dose validation,
  the 4-day grace period, live-vaccine spacing, proof of immunity, exemptions, earliest valid
  dates, and a roster mode that writes student results to `/data/output` and returns suppressed
  counts.
- `analyze_vaccination_coverage` (VAX-05): DPH school survey and CDC NIS/VaxView snapshots,
  herd-immunity thresholds with citations, school and town rankings, and local school counts.
- `vaccine_temperature_excursion_helper` (VAX-06), `lookup_vaccine_product` (VAX-07),
  `plan_vaccination_clinic` (VAX-08), and `vaccine_confidence_conversation_guide` (VAX-09).
- `lookup_medication_information` (CORE-07): RxNorm, openFDA (labels, recalls, shortages), and
  DailyMed; optional `PHN_OPENFDA_API_KEY`.
- New setting `vaccine_recommendation_basis` (in `configure_jurisdiction`), defaulting to DPH,
  then the professional societies, then CDC/ACIP.
- Immunization content under `content/vax/` (pending review), CDC CVX/MVX/trade name/NDC, DPH
  school survey, and NIS snapshots with their builder scripts in `xtask/scripts/`, and a
  fictional school roster in the synthetic corpus.
### Added — Phase 6: immunization forecasting and surveillance feeds

- `phn-cdsi` crate: CDC Clinical Decision Support for Immunization engine (Logic Specification
  v4.6) with the CDSi Supporting Data v4.65 embedded and checksummed; runs all 1,401 CDC test
  cases (all 1,064 healthy childhood and adult cases pass; 36 older underlying-condition cases
  are documented exceptions). Results: `docs/cdsi-test-results.md`.
- `forecast_immunizations_due` (VAX-03): dose evaluation, forecast dates, and a projected
  catch-up schedule for one person, or suppressed counts with per-person files written to
  `/data/output` for a file of many people.
- `get_notifiable_disease_trends` (CD-08): NNDSS weekly counts (data.cdc.gov `x9gk-5huc`)
  for Massachusetts, New England, and the U.S. compared with the five previous MMWR years (year
  to date and last 4 weeks), provisional year-end totals, and an all-conditions overview.
- `get_respiratory_and_wastewater_surveillance` (CD-09): weekly COVID-19, influenza, and RSV
  briefing from NHSN, NSSP, NREVSS, the ARI activity level, and NWSS wastewater levels by
  Massachusetts site, with data dates and lag, MWRA's final Deer Island values, and DPH's
  dashboard link.
- Bundled snapshots for both (about 260 KB), built by the new
  `cargo xtask build-surveillance-snapshots`; `cargo xtask build-snapshots` now keeps manifest
  entries it does not build.
### Changed — reference content re-checked against live sources

- Reportable conditions (`content/cd/reportable_conditions.yaml` 0.2.0) re-checked against the
  text of 105 CMR 300.000 (Mass. Register #1470, 5/27/22) and DPH's current provider and
  laboratory reporting lists: corrected timeframes, reporters, and recipients (for example,
  pertussis, mumps, brucellosis, glanders, melioidosis, and Q fever are not on DPH's
  immediate-call list; invasive group A strep, tetanus, HUS, and active TB are; TB, HIV, and
  STIs go directly to DPH); added 35 conditions, including alpha-gal syndrome (temporary
  declaration from April 1, 2026); removed leptospirosis and Candida auris, which the regulation
  does not name. Every entry now has a `citation`, shown in `lookup_reportable_condition`
  answers. Still pending subject-matter review.
- `lookup_reportable_condition` now tells providers to call the DPH Epidemiology Line for
  immediately reportable conditions and names the board of health where the case is diagnosed
  (not the town of residence), as 105 CMR 300.100 says.
- HMCC regions: Wilmington moved from 4AB to Region 3 to match DPH's HMCC Regional Boundaries
  map; the snapshot builder applies an override table.
- Data source registry: fixed the 105 CMR 300.000 address (the old mass.gov URL returns 404).

## [0.2.0] — 2026-11-07 (not tagged) — Phase 1: local data, privacy guardrails, and documents

### Added
- `read_local_file` (CORE-04): lists `/data` and extracts text and tables from PDF, DOCX, XLSX,
  XLS, ODS, CSV, HTML, Markdown, and text, with identifier warnings and redaction.
- `search_agency_policies` (CORE-05): full-text search of the agency's policies with file and
  section or page citations; index rebuilt automatically when files change.
- `export_document` (CORE-06): Markdown to accessible DOCX and tagged PDF with letterhead, XLSX,
  CSV, SVG, and PNG; local mail-merge.
- `apply_small_cell_suppression` (EPI-06): primary and complementary suppression.
- `profile_local_dataset` (EPI-07): structure, types, missingness, and identifier warnings;
  optional suppressed value summaries.
- `deidentify_dataset` (EPI-08): HIPAA Safe Harbor de-identification or limited data set, with a
  rare-combination check and a written report.
- `check_document_accessibility` (COMM-05): Section 508 / WCAG 2.1 AA checks for DOCX, HTML, PDF.
- PHI detector using column names, value patterns, and U.S. Census surname and SSA first-name
  lists; automatic strict redaction of documents that contain client identifiers.
- Synthetic test corpus with 266 seeded identifiers and MCP integration tests proving that no
  row-level data is returned by default and that exported documents pass the server's own
  accessibility checks.

## [0.1.0] — 2026-11-07 (not tagged) — Phase 0: foundation (internal)

### Added
- Rust workspace (`phn-core`, `phn-content`, `phn-data`, `phn-epi`, `phn-local`, `phn-docs`,
  `phn-tools`, `phn-server`, `xtask`) and an `rmcp` stdio server.
- Response envelope with markdown summary, structured data, sources, caveats, and privacy block;
  input and output JSON Schemas and annotations on every tool.
- Toolsets and role profiles, switchable at runtime with a `tools/list_changed` notification.
- Configuration through environment variables and `/cache/phn-settings.toml`.
- HTTP client with proxy and custom CA support, SQLite cache, live → cache → snapshot fallback,
  and a checksummed snapshot store.
- Massachusetts reference snapshots built from MassGIS: 351 municipalities with county, DPH
  preparedness region, and HMCC region; 402 school districts.
- Reference content format (YAML + JSON Schema) with review metadata; 105 CMR 300.000 reportable
  conditions (draft, pending review) and the data source registry.
- Tools: `get_data_source_status` (CORE-08), `configure_jurisdiction` (CORE-09),
  `calculate_rates` (EPI-01), `lookup_reportable_condition` (CD-01).
- Dockerfile (distroless, non-root, read-only root filesystem), GitHub Actions for CI, releases
  (multi-architecture GHCR image, SBOM, vulnerability scan, cosign signature, offline tarballs),
  and weekly snapshot freshness checks.
