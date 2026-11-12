## Changelog

- `appraise_evidence` (EBP-03): guided critical appraisal of a study the nurse describes or
  supplies (pasted text, or a TXT, Markdown, DOCX, PDF, or HTML file in `/data`, scanned with the
  PHI detector first; a file with a Social Security number, date of birth, or medical record number
  is refused, and the text is never returned). It suggests the design from words in the text (with
  the matched words and a strength; the nurse confirms with `design`), then lists that design's
  questions, each with its source and section: the 2011 Cochrane risk-of-bias domains plus CONSORT
  2010 reporting items (randomized trials), the seven ROBINS-I bias domains (non-randomized
  intervention studies), STROBE-based prompts (cohort, case-control, cross-sectional), the 16
  AMSTAR 2 items (systematic reviews), six appraisal questions from a health-education guide plus
  findings and limitations (qualitative), and SQUIRE 2.0 prompts (quality improvement reports).
  The tool never answers a question: it records the nurse's answers (checked against each
  question's scale), lists the ones to look at again, and applies each instrument's own overall
  rule where one exists (Cochrane table 3 summary risk of bias, ROBINS-I table 2, AMSTAR 2 box 2
  with its seven critical domains); reporting guidelines get no total. For a body of evidence on
  one outcome it adds the nurse's GRADE domain judgments (risk of bias, inconsistency,
  indirectness, imprecision, publication bias; large effect, dose-response, plausible confounding)
  to the starting level (high for randomized trials, low for observational studies) and takes an
  optional final rating with its reason; for a qualitative finding it records GRADE-CERQual
  concerns and the nurse's overall confidence with its definition. Optional Crossref DOI lookup
  (only the DOI is sent; an optional `PHN_CROSSREF_MAILTO` environment variable adds the
  nurse's own address for Crossref's polite pool), with the DOI as entered as the offline
  fallback and a statement of which was used. A Word record of the answers goes to `/data/output`.
  CASP is linked with its license terms flagged, not copied.
- `build_evidence_synthesis_table` (EBP-04): a synthesis table from the studies the nurse
  appraised (citation, design, population, sample size, intervention, comparator, outcome, effect as
  reported, quality rating and its tool) with a summary (studies, total and median sample size,
  studies by design, by rating and tool), as Word and Excel in `/data/output`; the Excel workbook
  has a live share-of-total formula, a Summary sheet of live COUNTA, COUNT, SUM, MEDIAN, and
  SUMPRODUCT formulas, and a PRISMA counts sheet whose derived counts are live formulas with OK or
  CHECK flags. From the counts the nurse enters it draws a PRISMA 2020 flow diagram (Mermaid text,
  a self-contained accessible SVG with title and description, a PNG in the Word file, and the
  diagram in sentences) after checking the arithmetic; every error is reported at once as a tool
  error and nothing is written.
- Crossref adapter (`phn_data::crossref`, `api.crossref.org/works/{doi}`): one-week cache,
  response validated before caching, DOI normalized (`https://doi.org/`, `doi:` prefixes).
- Content: `content/ebp/appraisal_checklists.yaml`, `content/ebp/grade_certainty.yaml`, and
  `content/ebp/evidence_synthesis.yaml` (with schemas), all `status: pending`.

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Here is an abstract (`study_abstract.txt`). What kind of study is it and what should I check?" | `appraise_evidence` (`path`) | Suggests a randomized trial ("likely") with the matched words (for example "randomly assigned", "allocation concealment"); lists 13 questions each with a source; returns none of the abstract's text and none of the corresponding author's name, email, or phone; says the review is pending and that the questions are not CASP's; links CASP and flags its license as unverified. |
| 2 | "Appraise this trial. Sequence generation, concealment, and blinding of assessors are low risk; blinding of participants is high risk because it is a behavioral program; attrition and reporting are low; nothing else." (with `design` set) | `appraise_evidence` (`design`, `answers`) | Overall summary risk of bias is high (one key domain high, per the 2011 Cochrane summary rule); the blinding answer is listed under "look again"; a Word record is in `/data/output` with the answers; the tool did not fill in any answer you did not give. |
| 3 | "Appraise this systematic review. It has a protocol, a good search, duplicate selection, but no list of excluded studies, and it never discusses risk of bias." | `appraise_evidence` (`design` `systematic_review`) | Two critical flaws (items 7 and 13) give "critically low" overall confidence while other items are unanswered; caveat that AMSTAR 2 advises against a score and that partial yes is not counted. |
| 4 | "Rate the certainty of evidence for emergency visits across four cohort studies: serious risk of bias, imprecision serious, otherwise no concerns." | `appraise_evidence` (`grade`) | Starts low (observational), sum -2, computed "very low"; asks for a reason if you give a different final rating; says GRADE applies to a body of evidence. |
| 5 | "I pooled three qualitative studies. Adequacy of data is a moderate concern and methodological limitations minor; I'd call it low confidence." | `appraise_evidence` (`cerqual`) | Records two components, lists the other two as not assessed, shows the CERQual definition of low confidence; does not compute the level itself. |
| 6 | "Look up DOI 10.1136/bmj.n71 and show me the checklist for a systematic review." (once online, once offline) | `appraise_evidence` (`doi`, `design`) | Online: citation details from Crossref cited with mode and date; offline: "doi_as_entered" and a caveat saying Crossref was not used. Only the DOI is sent. |
| 7 | "Appraise client_note.txt." | `appraise_evidence` (`path`) | Tool error: the file looks like a client record (categories named, no values). Same for a line list (not a study document). |
| 8 | "Make an evidence table for these six studies and give me Excel too." | `build_evidence_synthesis_table` | Table and summary in the chat; Word and Excel in `/data/output/evidence_synthesis/`; open the Excel file, change a sample size, and the Summary sheet and share-of-total column update. |
| 9 | "Draw a PRISMA flow diagram: 1,333 records, 241 duplicates, 1,092 screened, 1,050 excluded, 2 reports not retrieved, 40 assessed, 34 excluded (20 wrong population, 14 no comparison group), 6 included." | `build_evidence_synthesis_table` (`prisma`) | Counts accepted (reports sought 42 derived and listed under defaults); Mermaid text, SVG, and the sentences; open the SVG in a browser and with a screen reader or inspect its title and description; the Word file has the picture and its text alternative. |
| 10 | "Same, but say 1,000 were screened and 50 were assessed." | `build_evidence_synthesis_table` | Tool error naming each wrong number (screened should be 1,092; assessed should be 40) and writing no files. |

## Content review

- `content/ebp/appraisal_checklists.yaml` (review due 2027-02-01). Check each question against
  its cited source and section: the questions are this project's own wording of items from the
  Cochrane 2011 risk-of-bias paper and CONSORT 2010 (boxes on item changes; the checklist table
  itself did not load), ROBINS-I (domains, not signalling questions), STROBE explanation and
  elaboration, AMSTAR 2 (item rationale; the supplementary user guide was not read), one
  health-education paper on appraising qualitative research, and SQUIRE 2.0. Low or unverified:
  CASP was not read (site blocked), so its license (believed CC BY-NC-SA) and its list of
  checklists are marked confidence low; the 2011 Cochrane tool has been superseded by RoB 2
  (not read); how AMSTAR 2 counts a "partial yes" is unverified, so partial yes is listed and not
  counted; the design identification word lists are this project's heuristics (confidence
  medium); the qualitative questions follow a single guide, not a published instrument (medium);
  questions marked `confidence: medium` in the file are the ones whose fit to the source needs a
  second reader.
- `content/ebp/grade_certainty.yaml` (review due 2027-02-01). GRADE content comes from PubMed
  abstracts of the GRADE guidelines series (2011-2013) and Core GRADE 1-7 (BMJ 2025), because the
  GRADE Handbook and the BMJ pages were blocked and those papers are not in PubMed Central; GRADE
  guidance 24 and the GRADE-CERQual series were read in full. Not stated in the text read, so
  `confidence: low`: the levels to rate up for a dose-response gradient (set to one) and for
  opposing plausible confounding (set to one); the maximum of two levels to rate down in domains
  other than indirectness; the wording of the four certainty levels beyond "confidence that the
  estimate of effect is correct". Check these against the current GRADE Handbook and Core GRADE.
- `content/ebp/evidence_synthesis.yaml` (review due 2027-02-01). The flow elements and the
  record, report, and study definitions are from PRISMA 2020 (statement and explanation and
  elaboration item 16a, read in full). The figure is an image and could not be read, so the
  diagram layout is this project's; compare it with the official template and decide whether to
  add the "other methods" column and the previous-studies box. The table layout and sheet
  formulas are this project's design.

## Open items

- **CASP.** casp-uk.net, www.bmj.com, gdt.gradepro.org, doi.org, api.crossref.org, europepmc.org,
  prisma-statement.org, www.ncbi.nlm.nih.gov, pmc.ncbi.nlm.nih.gov, and the other hosts probed
  were blocked; only the NCBI E-utilities (PubMed abstracts and PubMed Central full text through
  `efetch`) worked. Decide whether the nurse wants the CASP checklists bundled after reading
  their license directly; until then the tool links to them.
- **Crossref was never reached.** The adapter follows Crossref's documented response format and
  is tested against a local server and a response written from the documentation. Run
  `appraise_evidence` with a DOI from a network that allows `api.crossref.org` before relying on
  it, and check the author, title, and year fields. Set `PHN_CROSSREF_MAILTO` (the nurse's own
  address, in the server's environment) for Crossref's polite pool; no address is ever sent
  otherwise.
- **Levels of evidence.** The tool does not assign a Melnyk or Oxford level-of-evidence number
  (those hierarchies are from books or sites that could not be read); it gives the GRADE starting
  certainty by design instead. Decide whether the nurse wants a hierarchy added from a source
  they can cite.
- **Design suggestion is word matching.** It can be wrong when a review of randomized trials
  mentions "randomized" many times (a systematic review word list outweighs it) or when a study
  mentions several designs. The nurse's `design` always wins, and answers require it.
- **Not built:** other designs (diagnostic accuracy, economic evaluation, case series, guidelines,
  mixed methods) have no questions; there is no QUADAS 2, RoB 2, or AGREE II; the PRISMA diagram
  omits the "other methods" column and the previous-studies box; the synthesis table does not read
  a CSV or XLSX of studies (name columns in a literature matrix trip the PHI column scan; typed
  rows only); no APA 7 reference formatting (brainstorming.md lists it for EBP-04); no PMID
  lookup.
- **XLSX formulas** were checked by recalculating the workbook in LibreOffice (headless) and by
  the cached values the tests read back; there is no test that parses the formula text because
  `phn-server` has no zip reader among its dev-dependencies.
- **Network lesson.** E-utilities `efetch` with `db=pmc` returns the full text of open-access
  papers when a paper has a body; older BMJ items and several STROBE and CONSORT tables come back
  without their tables (images). `db=pubmed` returns abstracts for journals not in PubMed
  Central (J Clin Epidemiol GRADE series, BMJ Core GRADE). YAML 1.1 reads `yes`, `no`, and `n` as
  booleans: quote them in content files (the schema check caught this).
- **Leak test.** Added `fixtures/synthetic/data/study_abstract.txt` (a fictional abstract with a
  fictional corresponding author) and its four seeded identifiers.
