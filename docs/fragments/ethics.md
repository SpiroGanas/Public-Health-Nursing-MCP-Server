# Fragment: Phase 18 `ethics` (EBP-07, POL-05, POL-06)

## Changelog

- `qi_vs_research_determination` (EBP-07, toolset `ebp`): a screening aid that sorts a project into
  quality improvement, program evaluation, public health practice or surveillance, or possible human
  subjects research. The nurse answers 14 questions (yes, no, or unsure) drawn from the definitions of
  research and human subject in 45 CFR 46.102 (including the 2018 public health surveillance
  exclusion), CDC's Guidelines for Defining Public Health Research and Public Health Non-Research
  (1999 revision), and two quality improvement ethics sources read as abstracts (a Hastings Center
  group, Ann Intern Med 2007; criteria proposed in JAMA 2000). Every question shows its reasoning and
  citation. The tool never answers for the nurse: with no answers it lists the questions; without a
  yes or no on the three required questions (intent to generalize, new or untested intervention, extra
  burden) it gives no reading; a provisional reading (not a determination) names the category, whether
  the project would involve human subjects under 46.102(e), and cautions for overlap with research,
  authority plus intent to generalize, a research component, publication plans, and unsure answers.
  Next steps include 244 CMR 9.03(18). It writes a draft project description and IRB determination
  request (DOCX with `[NURSE: ...]` placeholders and the answers table) to `/data/output`. Content
  `content/ebp/qi_research_determination.yaml` (`status: pending`, `review_due: 2027-02-01`). It does
  not describe any Massachusetts institution's IRB policy.
- `public_health_ethics_analysis` (POL-05, toolset `policy`): structures an ethics analysis of
  quarantine or isolation, exclusion from school, allocation of scarce resources, or another decision.
  It supplies Kass's six questions with prompts and facts to gather (and lists the ones the nurse has
  not answered), her three burden categories and balancing criteria, CDC's three functions of ethics
  inquiry, a table of options from least to most restrictive plus the nurse's own options for
  assessment, a justification checklist (effectiveness, proportionality, necessity, least infringement,
  public justification, fair procedure, scarce resources), the six core values and four-step analysis
  approach of the APHA Public Health Code of Ethics (2019), and a DOCX worksheet with `[NURSE: ...]`
  placeholders in `/data/output`. It never recommends a decision and does not cover Massachusetts law.
  Sources are paraphrased and cited, with copyright flagged. Content
  `content/policy/public_health_ethics.yaml` (`status: pending`, `review_due: 2027-02-01`).
- `nursing_scope_of_practice_guide` (POL-06, toolset `policy`): finds the provisions that bear on a
  scope, delegation, or standing-order question in a partial snapshot (2026-11-12) of 244 CMR 3.00,
  9.00, and 10.00, parts of 244 CMR 4.00, M.G.L. c. 112, section 80B, the Board of Registration in
  Nursing's decision-making guidelines and flow chart, and eleven advisory rulings. Each of the 59
  provisions has a citation, a quotation of 25 words or fewer (machine-checked against the fetched
  text), a paraphrase, the roles it applies to, and a confidence level; a topic index (15 topics)
  matches the question; the other 23 listed advisory rulings appear by title only. It never says that
  an act is within a nurse's scope: it shows citations and the Board's six questions, states its
  coverage in every result, and refers to the Board. Content
  `content/policy/nursing_scope_of_practice.yaml` (`status: pending`, `review_due: 2027-02-01`).

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Is our clinic's immunization reminder project QI or research?" (give no answers first) | `qi_vs_research_determination` | Lists 14 questions with sources, status `questions_only`, no reading; a DOCX template with `[NURSE: ...]` placeholders is in `/data/output`; the summary says the IRB decides. |
| 2 | Answer: generalizable intent no, new intervention no, extra burden no, local improvement yes. | `qi_vs_research_determination` (`answers`) | Provisional reading "quality improvement (likely not research)", flagged as not a determination; human subjects "not applicable"; next steps include writing a rationale; the DOCX holds the answers table and the reading. |
| 3 | "We are piloting a new home visiting approach and comparing it with usual visits; we plan to publish." | `qi_vs_research_determination` | After answers (intent yes, new intervention yes), reading "possible human subjects research (ask the IRB)" with research signals listed, the human subject questions raised, and 244 CMR 9.03(18) among the next steps. |
| 4 | Answer only "local improvement: yes" and ask for a reading. | `qi_vs_research_determination` | `needs_answers` listing the three required questions; no category is given. |
| 5 | "Our board of health is investigating a measles outbreak and collecting extra exposure questions to test hypotheses for other towns." | `qi_vs_research_determination` | `beyond_reporting` yes and `public_health_authority` yes gives "possible research", not "public health practice"; the caution cites CDC on added data collection. |
| 6 | "Walk me through the ethics of excluding unimmunized students for a measles outbreak." | `public_health_ethics_analysis` | School exclusion profile; Kass's six questions with the open ones listed; options from information to closure with burden prompts; checklist; no recommendation; the worksheet is in `/data/output`; the caveats say the APHA code could not be read. |
| 7 | Give responses to questions 1 and 2 and two alternatives of your own. | `public_health_ethics_analysis` (`responses`, `alternatives`) | Questions 1 and 2 are marked answered, 3 to 6 open; your alternatives appear at the end of the table with no level; the worksheet has your text and `[NURSE: assess]` cells. |
| 8 | "We have 400 vaccine doses and 2,000 eligible people. Structure the fairness analysis." | `public_health_ethics_analysis` | Scarce-resource profile; justification checklist and APHA values; no allocation is proposed. |
| 9 | "Can an LPN give flu vaccines at a community clinic under a standing order?" | `nursing_scope_of_practice_guide` | Cites AR 9804 (vaccines may be given from a standing order protocol; required elements) and AR 9324 (standing order limits), with quotations; the Board's six questions; coverage says partial; no "yes, it is within scope"; the Board's phone number appears. |
| 10 | "What must I do before delegating a task to an unlicensed health aide?" | `nursing_scope_of_practice_guide` | 244 CMR 3.05(2) to (5) provisions and AR 9803's five rights; health teaching may not be delegated; medication delegation flagged with the PDF layout note. |
| 11 | "Can a nurse do infusion therapy?" | `nursing_scope_of_practice_guide` | No provision matches; AR 9204 is listed by title as not read; the topic index is shown; the caveat says no match does not mean no rule applies. |
| 12 | "What do the Board's rules say about naloxone standing orders?" | `nursing_scope_of_practice_guide` | AR 24-01 (no discipline solely for harm reduction supplies; DPH standing order for pharmacists; nurses may use standing orders under AR 9324) with the note about the ruling's number. |

## Content review

- `content/ebp/qi_research_determination.yaml` (the nurse should read against 45 CFR 46.102 on the
  eCFR and CDC's 1999 guidelines, and check the questions and the rules that turn answers into a
  reading). High confidence: the regulation definitions and the CDC statements (read in full). Medium
  confidence: the Hastings Center (Lynn 2007) and JAMA 2000 (Casarett) points, read as PubMed abstracts
  only. The 14 screening questions, the category descriptions, and the rules that combine answers are
  this project's synthesis (medium). Check which questions are required (generalizable intent, new
  intervention, extra burden) and whether to add the OHRP decision charts and the institution's own
  questions. Not verified: OHRP's QI FAQ (hhs.gov blocked), any Massachusetts institution's IRB policy,
  forms, or who may make a not-research determination.
- `content/policy/public_health_ethics.yaml`: the six Kass questions, burden categories, balancing
  criteria, and Childress justification conditions come from an open-access overview (BMC Public Health
  2010) and the PubMed abstract of Kass 2001 (medium). The APHA 2019 code's six core values come from
  two open-access papers (medium); its four-step analysis approach and the "additional considerations"
  come from one paper (low). The APHA code itself (apha.org) and Kass's full article could not be read:
  check wording and section structure against the originals. The decision profiles (quarantine and
  isolation, school exclusion, scarce resources, general), their tensions, facts to gather, and ordered
  options are drafting aids, not sourced (low); edit them for Massachusetts practice, and check that
  nothing in them contradicts 105 CMR 300 or DPH guidance.
- `content/policy/nursing_scope_of_practice.yaml` (59 provisions, 11 rulings read, 23 listed by title):
  every quotation was checked verbatim against the fetched text (script in the helper's notes; not
  committed because the sources are not). Medium confidence items: the 244 CMR 3.05(5)(d) medication
  delegation requirements (the mass.gov PDF omits a lead-in and prints the seven numbered requirements
  on the page before the paragraph; check the official text), the 244 CMR 4.00 entries (read in part;
  the category list in the PDF text was garbled), AR 25-01's definition of "groups" (the ruling's
  footnote cites a scoping review; ask the Board how it applies to population-level public health
  work), and the Board's decision question 2 (the list of "factors" did not survive text extraction;
  the flow chart words it as consistency with nursing literature and research). Not read: 244 CMR 5.00
  to 8.00, M.G.L. c. 112 sections 74 to 81C except 80B, and the other advisory rulings.

## Open items

- **Pending review, partial sources.** All three content files are `status: pending`,
  `review_due: 2027-02-01`. The nurse decides whether to extend the scope snapshot (more advisory
  rulings; 244 CMR 4.00 in full; 244 CMR 7.00 on complaints) or leave the rest to the Phase 19 law
  corpus (M.G.L. c. 111, 105 CMR, 244 CMR). Advisory rulings are revised over time; the snapshot has
  no `--check` builder, and mass.gov pages carry dynamic markup, so a refresh is manual. A small
  script that re-downloads the 11 ruling PDFs and compares text would be the first step.
- **Decision for the nurse:** whether the three required QI questions are the right minimum, whether
  an "undetermined" reading is acceptable, and whether to add a question on whether the institution's
  assurance applies to all research (kept informational now).
- **Technical debt:** `read_answers` encodes the rules by question id (documented in the tests); a
  change to the question list needs a matching change in the code. `match_topics` is keyword matching
  with a trailing-s stem, not search; phrasing outside the keyword lists finds nothing (the result then
  shows the topic index and says no match is not an answer). The tools write DOCX only (no PDF).
- **Network lessons.** Reachable: the eCFR versioner API (`/api/versioner/v1/full/<date>/title-45.xml`
  needs `Accept-Encoding`, so use `curl --compressed`), cdc.gov's scientific-integrity pages and PDFs
  (the old `/os/integrity` paths 404; the 1999 guidelines are at
  `/scientific-integrity/media/pdfs/2024/10/defining-public-health-research-non-research-1999.pdf`),
  mass.gov regulation and advisory-ruling documents (plain curl, `/doc/<slug>/download`), and
  malegislature.gov. NCBI E-utilities efetch for `db=pmc` returns full text only for open-access
  articles (Kass 2001 itself is refused with "publisher does not allow downloading"; open-access
  papers that tabulate its questions work). Blocked: apha.org, phls.org, ccnpps-ncchpp.ca,
  pmc.ncbi.nlm.nih.gov and www.ncbi.nlm.nih.gov web pages, europepmc.org, hhs.gov (OHRP). WebFetch
  cannot reach hosts the container proxy blocks.
- **PDF text quirks.** The mass.gov 244 CMR PDFs have two overlapping text layers; `pdftotext -raw`
  gives the cleaner one, with some missing spaces. Verify any quotation against the page image.
