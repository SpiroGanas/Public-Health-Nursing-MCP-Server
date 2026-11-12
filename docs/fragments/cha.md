# Fragment: Phase 15 `cha` (EQ-08, EQ-09, EQ-10, and the `community_health_assessment` prompt)

## Changelog

- Community health assessment tools (toolset `equity`):
  - `community_health_assessment_builder` (EQ-08): guides a MAPP 2.0 community health assessment for
    a town or county. Lays out the three phases and three assessments (verified against two 2025
    journal articles; NACCHO's own pages were unreachable, and the result says so). Assembles the
    community status assessment by calling the EQ-01 profile (with CDC PLACES and the Social
    Vulnerability Index), EQ-03 language access, EQ-02, and the Phase 14 environmental justice,
    climate, food access, drinking water, and fluoridation tools as library functions (a section
    that fails is reported with its reason and the rest still return). Adds a 13-sector partner
    checklist (a partner roster is counted by sector; organizations only, contacts never read),
    context assessment prompts, nearby hospitals from a new CMS snapshot (`ma_hospitals.csv`, 84
    Massachusetts hospitals), the federal CHNA rule (26 CFR 1.501(r)-3, read in the eCFR) and the
    Attorney General's Community Benefits Guidelines (read in full), links to find each hospital's
    CHNA, and the priority lists (with page citations and shared themes) found in CHNA PDF, DOCX,
    HTML, or TXT files the nurse saves in `/data`. Writes draft report sections to DOCX or PDF.
  - `prioritize_health_issues` (EQ-09): ranks community health issues with the Hanlon method
    (BPR = ((A + B) x C / 3) x D, with the PEARL test as a multiplier, a pre-screen, or off), a
    strategy grid (importance by feasibility), and nominal group scoring from vote totals. Ties
    share a rank and are listed, missing ratings are listed as still needed (never zero), close
    calls are flagged, and the method and rationale are written to DOCX or PDF. Arithmetic is
    tested against hand-computed values and a published case study.
  - `analyze_qualitative_data` (EQ-10): reads focus group and key informant transcripts locally,
    removes names and other identifiers (speaker labels become P1, P2, ...; names from the labels
    and an optional names file; capitalized words inside sentences; the PHI detector's patterns),
    codes them with the nurse's own codebook (CSV, XLSX, or YAML) or a draft starter codebook, and
    returns counts only: a scrub report, the codebook, and for each theme the number of different
    participants who raised it. A code raised by fewer than three participants (or by no one) is left out
    entirely. The de-identified coded excerpt matrix (XLSX) and a theme summary (DOCX) go to
    `/data/output`; no transcript sentence, speaker label, or file name is ever returned.
- MCP prompt `community_health_assessment`: a MAPP 2.0 workflow that chains EQ-01 to EQ-10 and the
  Hanlon calculator, names the toolsets it uses (`equity`, `chronic`, `env`), and keeps transcripts
  and client files in the data folder.
- Content: `content/equity/cha_framework.yaml`, `prioritization.yaml`, and
  `qualitative_codebook.yaml` (all `status: pending`); snapshot `ma_hospitals.csv` with builder
  `xtask/scripts/build_cha_snapshots.py` (`--check` mode); synthetic transcripts and a names file in
  `fixtures/synthetic/data` with their planted identifiers.

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Start a community health assessment for Lowell." | `community_health_assessment_builder` | Three MAPP 2.0 phases and three assessments, with a statement that MAPP 2.0 was checked against journal articles and not NACCHO's handbook (Phase 3 shown as low confidence); eight data sections, each "included" or with a reason; Lowell General Hospital listed as "in the town"; links to the Attorney General's reports site and the guidelines; the federal rule cited as 26 CFR 1.501(r)-3; a DOCX in `/data/output`; the data sources show live, cache, or snapshot with dates. |
| 2 | "Do a community health assessment for Hampden County, with only the profile and the fluoridation section." | `community_health_assessment_builder` (`sections` profile, fluoridation) | The fluoridation section says "not for this level" (a county) instead of failing; hospitals in Hampden County are listed as "in the county". |
| 3 | "Read the hospital CHNAs I saved in /data/chna and tell me what each one prioritized." (Save two real CHNAs first, one PDF and one Word.) | `community_health_assessment_builder` (`chna_paths`) | For each file: page count, the year, the priority lines with the page where each was found, and themes; open the reports and check the lines against the executive summaries; lines that are wrong or missing are noted for the content review. No staff names or phone numbers appear. |
| 4 | "Does my partner list cover the right sectors?" (Use `partners.csv` with organization and sector columns and a contact person column.) Then repeat with `hrsn_screenings.csv` and `line_list.csv` as the partner list. | `community_health_assessment_builder` (`partners_path`) | Counts of organizations by sector and the sectors with none; the contact people, phone numbers, and emails never appear. The two client files are refused as lists of people without echoing a value. |
| 5 | "Use client_note.txt and line_list.csv as CHNA reports." | `community_health_assessment_builder` (`chna_paths`) | The note is refused as a client record; the spreadsheet is refused as not a document; no identifier appears in either error. |
| 6 | "Rank these issues: housing cost burden (size 8, seriousness 14, effectiveness 6), food insecurity (5, 10, 6), transportation (6, 9, 6), opioid overdose (3, seriousness 17, effectiveness 7, fails the legality question), and dental care (size 4, seriousness 6 only). All pass PEARL except where I said." | `prioritize_health_issues` | Housing 44.00 (rank 1); food insecurity and transportation 30.00, tied at rank 2 and listed as a tie, with no tie broken; opioid overdose 0.00 because PEARL failed, with its basic rating 46.67 shown; dental care listed as incomplete with what it needs, not ranked as zero; a DOCX with the method, ratings, and rationale in `/data/output`. Check one score by hand: (8 + 14) x 6 / 3 = 44. |
| 7 | "Screen out any issue that fails PEARL before ranking, and show the strategy grid." | `prioritize_health_issues` (`pearl_mode` prescreen) | Failed issues are listed as screened out and not ranked; each issue has a grid quadrant (act now, important but hard, quick win, revisit later) and the grid is described as this project's own. |
| 8 | "We held a nominal group: 10 people each ranked their top 3. Issue X got 4 first, 3 second, and 1 third place votes; Y got 3, 2, 4; Z got 2, 3, 3. Score it." Then "Y got 7 first-place votes" with the same totals. | `prioritize_health_issues` (`nominal_group`) | X 19 points, Y 17, Z 15, in that order; no names are asked for. The second request is refused because rank 1 would have been given more times than there are participants. |
| 9 | "Analyze the focus group in focus_group_transcript.txt. Use participant_names.txt to remove names." | `analyze_qualitative_data` | A scrub report (counts by kind), five participants, themes with participant counts (housing, transportation, and others raised by three or more), at least one theme withheld (substance use: one participant); no name, phone number, email, address, employer, or sentence from the transcript appears in the chat; `/data/output/qualitative_analysis*/` has the coded excerpt matrix and theme summary; open the matrix and check that excerpts are de-identified and that each excerpt matches its code. |
| 10 | "Analyze both transcripts together without a names file, and save the de-identified transcripts." | `analyze_qualitative_data` (`write_deidentified_transcripts`) | Nothing seeded appears in the chat or in the de-identified text files (names come from speaker labels and capitalization); read the de-identified files and note any remaining detail that could identify a person (for the content review). |
| 11 | "Use my_codebook.csv for the coding." (Save a codebook with code, definition, keywords.) Then try `min_voices` 2, `line_list.csv` as the transcript, and `line_list.csv` as the codebook. | `analyze_qualitative_data` | The nurse's codes replace the starter list; `min_voices` below 3 is refused with the reason; the spreadsheet is refused as not a document and the line list as not a codebook, with no values echoed. |
| 12 | Choose the "MAPP 2.0 community health assessment" prompt with area "Lowell". Follow it through the first three steps. | prompt `community_health_assessment` | Claude calls `community_health_assessment_builder` first, asks who the partners are and for a roster, names the toolsets (`equity`, `chronic`, `env`), says MAPP 2.0 was not checked against NACCHO's handbook, asks for transcripts to be saved in the data folder rather than pasted, and does not rank issues before the list is confirmed. |

## Content review

- `content/equity/cha_framework.yaml` (MAPP 2.0 and CHNA guidance). **NACCHO's MAPP 2.0 pages and
  handbook could not be fetched** (naccho.org returns a proxy 403), so MAPP 2.0 was verified through
  two open-access 2025 articles (Front Public Health 13:1601406 and Inquiry 2025; both cited in the
  file). Confirmed there (medium confidence): the three phase names, the three assessments with
  their data types and purposes, three Phase 1 tools (stakeholder and power analysis, Starting Point
  Assessment, vision), the Community Partner Assessment's 59-question survey and 14.5 to 17.5 hours
  of partner time, and that it replaced the Local Public Health System Assessment. **Low
  confidence:** what Phases 2 and 3 contain beyond a one-line description, where prioritization and
  the community health improvement plan fall, NACCHO's tool names, and that MAPP 2.0 centers equity
  (one sentence in one article). The nurse should check the phases against NACCHO's handbook, add
  the tools' exact names, and correct the mapping of this server's tools to the three assessments
  (this project's own). The 13 partner sectors, their keywords, the context prompts, the CHNA theme
  words, and the report outline are this project's drafts. The federal rule (26 CFR 1.501(r)-3,
  eCFR, current on 2026-11-12) and the Attorney General's Community Benefits Guidelines (February
  2018, 33 pages) were read in full (high confidence). The Attorney General's reports site address
  (`massago.hylandcloud.com/231cbs/annualreport.aspx`) is from the Attorney General's mass.gov page
  and the site itself could not be opened (medium). No per-hospital CHNA link is bundled.
- `content/equity/prioritization.yaml` (Hanlon, PEARL, nominal group, strategy grid). The formula
  BPR = ((A + B) x C / 3) x D, the component ranges, the four seriousness factors, and the PEARL
  questions were read in three open-access papers (Rev Panam Salud Publica 2019; Rev Saude Publica
  2017; J Public Health Manag Pract 2026; high confidence for what they say). **NACCHO's Hanlon
  worksheet could not be read**: whether it divides by 3, how it scores seriousness, and its
  anchors are unverified. The Illinois variant (effectiveness 0.5 to 1.5), the Vilnius and Dandoy
  benefit-cost variant, and PAHO's variant are described but not implemented. **No scoring anchors
  are bundled** (none were found). The strategy grid (importance = (A + B) / 30 by feasibility =
  C / 10, threshold 0.5, four quadrant names) and the nominal group point rule (N points for first
  place down to 1) are this project's own (low and medium confidence); the sources name strategy
  grids but do not describe one.
- `content/equity/qualitative_codebook.yaml` (starter codebook and scrub settings). Every code and
  keyword is a project draft with no source (low confidence). The rare-theme floor of three
  participants, the facilitator and non-speaker labels, the list of capitalized words the scrubber
  keeps, and the stopwords are tool defaults.
- `snapshots/ma_hospitals.csv` (CMS Hospital General Information, 84 rows, 2026-11-12): check the
  town mapping (Boston neighborhoods, Leeds to Northampton, Pocasset to Bourne, South Attleboro to
  Attleboro, West Concord to Concord; Devens is kept as CMS wrote it), recent openings, closures,
  and renamings (CMS lags), and that the "likely files a community benefits report" flag (non-profit
  acute care, critical access, or children's hospital) matches the Attorney General's filer list.

## Open items

- **Hospital CHNA links and the Attorney General's reports site were not reachable**
  (`massago.hylandcloud.com`, and every hospital site tried: massgeneral.org, bmc.org,
  baystatehealth.org, umassmemorial.org, lahey.org, mgbcommunityhealth.org). The tool therefore
  lists hospitals and tells the nurse how to find each CHNA, but bundles no per-hospital link and no
  list of which hospitals file. Decision for the nurse: build a verified directory of CHNA links from
  a machine that can reach those sites, or keep the search-by-hospital instructions.
- **Priority extraction is pattern-based** (bullets, numbered lists, short heading lines, an inline
  list after "priorities:", with PDF text where a bullet mark sits on its own line) and was tested
  on synthetic text and on a PDF generated by this project; it has not been run on real hospital
  CHNAs (they vary a lot: tables, infographics, scanned pages). Expect misses; the nurse should try
  three or four real CHNAs and send the layouts that fail.
- **MAPP 2.0 is checked against secondary sources only.** The nurse (or NACCHO's handbook) should
  confirm the phase contents, the assessment names, and the Community Partner Assessment tool, and
  decide whether the tool should reproduce or only link NACCHO's worksheets (their license terms were
  not read).
- **EQ-04 (disparities) is pointed to, not run:** it needs the nurse's own table of counts. The
  builder could accept a counts file later. Vital statistics are still not included.
- The builder runs the Phase 14 and EQ tools sequentially after the four network calls (profile,
  language, climate, water) run together; a live run can take about a minute. The excerpts it keeps
  from each tool's summary are cut at paragraph boundaries (1,800 characters).
- **Qualitative scrubbing is conservative and heuristic.** Capitalized words inside sentences that
  are not Massachusetts towns, counties, months, languages, or a short list of labels become
  `[NAME]` (organizations and places are over-removed); a name written in lower case, a nickname, or
  a unique detail will remain. Sentence-initial words are removed only if the word is a known name or
  appeared capitalized inside a sentence elsewhere in the same transcript. Speaker labels are only
  read as turns when at least two "Label:" lines exist. Decisions for the nurse: whether organization
  and place names should be kept (needs a reviewed list), whether three participants is the right
  floor for her agency's policy, and whether the de-identified transcripts should ever be written by
  default (they are off by default).
- A person who speaks in two transcripts counts as two participants (labels are not matched across
  files); themes from a single transcript with fewer than three speakers can never be reported.
- No OCR: scanned PDFs (transcripts or CHNAs) have no text and are refused with that explanation.
- Network lessons for STATUS.md: **naccho.org returns a CONNECT 403** (the whole MAPP 2.0 and Hanlon
  worksheet set was unreachable); NCBI E-utilities (`eutils.ncbi.nlm.nih.gov`, `efetch db=pmc`) return
  full text of open-access PMC articles when `www.ncbi.nlm.nih.gov/pmc` pages are blocked; the eCFR
  versioner API needs `--compressed` (`Accept-Encoding`) and gave the full text of 26 CFR
  1.501(r)-3; `data.cms.gov/provider-data/api/1/datastore/query/xubh-q36u/0` serves Hospital General
  Information (use `curl -g` for the bracketed conditions); mass.gov served the Attorney General's
  community benefits pages and guideline PDF; blocked: irs.gov, law.cornell.edu, healthypeople.gov,
  countyhealthrankings.org, phf.org, apha.org, archive.org, Wikipedia, and search.mass.gov.
- YAML gotcha found while writing content: a plain list item or value containing `": "` parses as a
  mapping, and a bare `yes` parses as a boolean (the content loader then fails with "invalid type:
  map" or "boolean"); quote such strings. PyYAML is more forgiving than the Rust loader, so test
  content through the Rust tests.
