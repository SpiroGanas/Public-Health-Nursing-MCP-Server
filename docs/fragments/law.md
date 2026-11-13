# Fragment: Phase 19 `law` (POL-02, POL-04, POL-07)

## Changelog

- `search_public_health_law` (POL-02, toolset `policy`): searches a bundled, dated (2026-11-12) corpus of
  Massachusetts public health law and answers with citations. **M.G.L. c. 111:** a section index of all 510
  sections that are not repealed (the Legislature's chapter page) and full text for 45 core sections (powers
  of the Department and boards of health, disease reporting, isolation and quarantine, nuisances and the
  sanitary code, the immunization registry, newborn screening, HIV and venereal disease record
  confidentiality). **105 CMR:** all 89 chapters on mass.gov's table of contents with page dates and links,
  a 3,060-section number and title index read from each chapter's PDF, and full text for 20 chapters (172,
  200, 201, 210, 220, 221, 222, 223, 270, 271, 300, 335, 360, 365, 400, 410, 460, 590, 661, 665). **244 CMR:**
  all eight chapters in full text (51 sections). A citation (`105 CMR 300.200`, `M.G.L. c. 111, § 95`, a
  whole chapter) returns the exact section or the chapter's sections; words are matched on titles and text
  with a coverage threshold. Every hit has the citation, title, link (and PDF link), the chapter date, the
  snapshot date, whether the section is `full_text` or `index_only`, and an excerpt of 25 words or fewer
  (machine-checked); 244 CMR hits point to the matching provisions in POL-06's content. Topic starting
  points (14 topics) list the key sections with titles. **Local regulations:** a folder under the data
  folder (default `local_regulations`) is indexed with the Phase 1 policy index; files with a date of birth,
  record number, or health plan number are refused (counted, never named), phone numbers, e-mail and street
  addresses in excerpts are masked, and spreadsheets are ignored. **Larger corpus:** `law_corpus/` in the
  data or cache folder (built with `xtask/scripts/build_law_snapshots.py --extended`, every c. 111 section
  and every 105 CMR chapter in full text) is used only when its manifest and SHA-256 checksums match
  (ADR 0011 pattern). The builder also refreshes the bundled snapshot (`--check` compares). Content
  `content/policy/public_law_search.yaml` and snapshots `ma_law_mgl_c111.json`, `ma_law_105_cmr.json`,
  `ma_law_244_cmr.json` (`status: pending`, `review_due: 2027-02-01`). Decision support, not legal advice.
- `draft_policy_brief` (POL-04, toolset `policy`): drafts a one-page or full policy brief (DOCX and/or
  Markdown in `/data/output`) in the order of the Eightfold Path of policy analysis: summary and request,
  problem, evidence, options, criteria (full), an options-by-criteria table with ratings in words (strong,
  mixed, weak, unknown) so it reads without color, trade-offs (full), recommendation, implementation and
  evaluation (full), references (full). Template + local fill: the nurse supplies the problem, evidence with
  citations (for example copied from the EBP tools' output), options, criteria, ratings, and recommendation;
  everything else is a `[NURSE: ...]` placeholder; the tool never invents facts. It reports the status of
  each of the eight steps, counts the placeholders left, and runs the existing readability analysis
  (`comms::readability::analyze`) on only the prose the nurse supplied. The steps are described from two
  open-access (CC BY 4.0) articles and two PubMed abstracts; Bardach's book was not read. The criteria menu
  has ten items: eight used in the published analyses read and two marked as this project's suggestions
  (low confidence). Content `content/policy/policy_brief.yaml` (`status: pending`, `review_due: 2027-02-01`).
- `health_information_sharing_guide` (POL-07, toolset `policy`): asks five structured questions (who holds
  the record and in what role, the kind of record, the recipient, the purpose, consent) and returns the
  regimes that may govern, ordered likely governs, may apply, check, not indicated, each with the reason,
  provisions (citation, link, 25-word machine-checked quotation, paraphrase, confidence), cautions, and the
  facts about the nurse's organization to confirm: HIPAA (45 CFR 160.103, 164.105, 164.501, 164.502, 164.506,
  164.512), FERPA (34 CFR 99.1, 99.3, 99.30, 99.31, 99.36), 42 CFR Part 2 (2.11 to 2.13, 2.32, 2.33, 2.51,
  2.54, as amended in 2024), the Massachusetts public records law (c. 66 section 10, c. 4 section 7 clause
  twenty-sixth), 105 CMR 300.120 and 300.191, c. 111 sections 24M, 70F, 111, 119, c. 112 section 12CC, and
  c. 71 sections 34D and 34E. It never gives a yes or no on whether a disclosure is allowed (`answer_type`
  `governing_rules_and_cautions`) and refers to agency counsel. Content
  `content/policy/information_sharing.yaml` (`status: pending`, `review_due: 2027-02-01`).

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "What does 105 CMR 300.200 say about isolation and quarantine?" | `search_public_health_law` | One hit, `full_text`, chapter date 2022-05-27, a link to the mass.gov chapter page and PDF, an excerpt of 25 words or fewer; coverage and the "not legal advice" caveat shown. |
| 2 | "Which sections of M.G.L. chapter 111 let a board of health order someone isolated or quarantined?" | `search_public_health_law` | Sections such as c. 111 sections 95 and 104 and 105 CMR 300.200 and 300.210 appear with titles and links; the topic starting points for isolation and quarantine are listed. |
| 3 | "Look up M.G.L. c. 111, section 143." | `search_public_health_law` | The section is `index_only`: title and link, no excerpt, and a caveat saying the text is not bundled. |
| 4 | "List the sections of 105 CMR 410." | `search_public_health_law` | A chapter lookup (State Sanitary Code, chapter II) listing sections in order, with `more_hits` when over the limit. |
| 5 | "Where does a nurse's authority to delegate come from in 244 CMR?" | `search_public_health_law` | 244 CMR 3.05 (full text) with a pointer to `nursing_scope_of_practice_guide`. |
| 6 | Put your town's tobacco regulation (PDF or Word) and a spreadsheet in `local_regulations/`, then: "Search our local regulations and the state rules for flavored tobacco." | `search_public_health_law` | Local hits (file and section) and 105 CMR 665 hits; the spreadsheet is ignored; no phone numbers or e-mail addresses from the regulation appear. |
| 7 | Add a visit note with a date of birth to `local_regulations/` and repeat. | `search_public_health_law` | The note is refused (counted in `refused_client_like_files`) and never named or quoted. |
| 8 | "Search for the quarantine order for Maria at 12 Pine Street." | `search_public_health_law` | Refused with a message that does not repeat the address. |
| 9 | Install the larger corpus as `law_corpus/` (built with the `--extended` script), then look up M.G.L. c. 111, section 143. | `search_public_health_law` | The section now has an excerpt and the source says `extended`; with one byte of a file changed, the bundled corpus is used and the caveat says the checksum did not match. |
| 10 | "Draft a one-page policy brief for the board of health on a rule for mold in rental housing. Options: adopt a rule, education only, continue as is." (give no facts) | `draft_policy_brief` | DOCX in `/data/output` with the eight steps listed, every fact a `[NURSE: ...]` placeholder, a three-row comparison table of placeholders, no numbers or citations the nurse did not give; readability check not run (too little text). |
| 11 | Give the problem, two evidence items with sources, three options, four criteria and six ratings, then ask for the full brief. | `draft_policy_brief` | Sections marked supplied or partly supplied; the table cells read "Strong: reason" and so on; placeholders counted; readability grade against the target with suggestions; Markdown and DOCX both written. |
| 12 | Give an evidence item without a citation. | `draft_policy_brief` | The item appears with "[NURSE: citation for this finding]" and the evidence section is marked partly supplied. |
| 13 | "A school asks for a child's immunization record held by our clinic. Which rules govern sharing it?" | `health_information_sharing_guide` | Asks for holder, record type, recipient, and purpose if not given; with them: HIPAA (45 CFR 164.512(b)(1)(vi), with the parent agreement condition) and 105 CMR 300.191(B) cited; FERPA may apply once the school keeps the record; no yes or no. |
| 14 | "I am a public school nurse. The health department wants a student's health office record for a disease investigation." | `health_information_sharing_guide` | FERPA likely governs, HIPAA not indicated (student health records are education records), c. 71 section 34E; FERPA consent and emergency provisions cited; counsel referral. |
| 15 | "Our board of health got a public records request for a disease investigation file." | `health_information_sharing_guide` | Public records law may apply; 105 CMR 300.120 and the statutory exemption cited; caution about 10 business days and counsel; no instruction to release or withhold. |
| 16 | "A substance use treatment program is asked to share records with a public health authority." | `health_information_sharing_guide` | Part 2 likely governs; 42 CFR 2.54 allows only de-identified disclosure to a public health authority; consent form caution (2.31 not read); court order and subpoena caution. |

## Content review

- `content/policy/public_law_search.yaml` and the three snapshots (`snapshots/ma_law_*.json`): the nurse should
  spot-check ten sections of each source against the Legislature's and mass.gov's pages (section numbers,
  titles, dates), confirm the 45 full-text c. 111 sections and 20 full-text 105 CMR chapters are the right
  core set, and review the 14 topic starting-point lists (selected by this project from section titles, not
  checked by a lawyer). Known weaknesses: regulation text comes from PDF extraction (tables, forms, and
  appendices may be missing; `105 CMR 63.00` has no section numbers and appears as a chapter only);
  mass.gov copies are unofficial and the chapter date is the date on the page; c. 111 has no per-section
  effective date. Nothing in the corpus is low confidence, but it is partial and every result says so.
- `content/policy/information_sharing.yaml`: 43 provisions with machine-checked 25-word quotations. Medium
  confidence: `hipaa_public_health_def` (added 2024 and amended since; status should be confirmed),
  `hipaa_judicial`, `hipaa_research`, `ferpa_studies`, `ferpa_treatment_records`, `part2_consent`,
  `ma_c111_24m`, `hipaa_personal_rep`, `ma_pr_definition`, `ma_pr_privacy_exemption` is high. The screening
  logic in `bears()` (which regime to read for a holder and record type) and the cautions are this project's
  drafting judgment and need review by counsel. **Not read:** the FERPA statute, HHS and Education joint
  guidance, 603 CMR 23.00, Part 2 subpart E and the 2024 rule's compliance dates (the "February 2026"
  compliance date is NOT in the content because it could not be verified), 45 CFR 164.508 and 164.520,
  M.G.L. c. 214 section 1B, c. 93H, c. 123 section 36, c. 111 sections 70E and 70G.
- `content/policy/policy_brief.yaml`: the eight step names are this project's short labels for the steps
  described by two open-access articles; the section prompts and the rating scale are drafting aids; the
  criteria `legal_authority` and `evidence_strength` are low confidence (no source read). Bardach's book was
  not read (4th edition, 2012, as cited by the articles). Check that the defaults (health impact, equity,
  economic impact, feasibility) suit board of health audiences.

## Open items

- **Blocked sources:** govinfo.gov, www.law.cornell.edu, the Secretary of the Commonwealth's CMR site
  (www.sec.state.ma.us, eli.sec.state.ma.us), the DESE site (doe.mass.edu), hhs.gov, and the Federal
  Register are blocked, so the official CMR text, 603 CMR 23.00, and HHS and Education guidance were not
  read. Working: malegislature.gov (HTML only; there is no JSON API), mass.gov regulation pages and PDFs
  (plain curl user agent), the eCFR versioner API (needs `Accept-Encoding`, so use `curl --compressed`;
  the `full` endpoint returns one section as XML), and NCBI E-utilities for PubMed and PMC.
- mass.gov's 105 CMR table of contents is on every chapter page; chapter URLs are
  `/regulations/105-CMR-<chapter x 100>-<slug>` and the PDF link is on the page. The 244 CMR PDFs hold every
  page twice (two text layers); `pdftotext` interleaves them, so the builder reads them with `pypdf` and keeps
  the second layer. `xtask/scripts/build_law_snapshots.py` needs `pdftotext` and, for 244 CMR, `pypdf`.
- Decisions for the nurse: which additional 105 CMR chapters and c. 111 sections to bundle in full text
  (the bundled 105 CMR snapshot is 966,000 bytes against a 1,000,000 byte cap; adding a chapter means
  dropping one); where agencies keep local regulations (the default folder name `local_regulations` is this
  project's choice); whether to add 105 CMR 130 to 180 (licensure) and 603 CMR 23.00 once reachable.
- Known limits: keyword matching is by words (plural endings removed), not meaning; there is no phrase
  search across chapters beyond a bonus for adjacent words; the local folder is indexed with the Phase 1
  index, which splits documents by headings or pages; HIPAA and Part 2 text was amended in 2024 and later
  and the guide cannot say what a court has done with those amendments.
- The leak test now runs the whole synthetic data folder through the local search as a regulations folder;
  files that carry a date of birth, record number, or health plan number are refused, and contact details
  are masked in what remains.
- Tool count: 140 with this package (137 plus three).
