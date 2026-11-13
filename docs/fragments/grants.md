# Fragment: Phase 17 `grants` (PLAN-06, PLAN-07, PLAN-08)

## Changelog

- `search_grant_opportunities` (PLAN-06, toolset `planning`): open funding opportunities for a
  health agency. Federal listings come from the Simpler.Grants.gov search API
  (`POST /v1/opportunities/search`, request and response shapes read from the project's published
  OpenAPI file) when the nurse sets the optional `PHN_GRANTS_GOV_API_KEY`; without a key, or when
  the request fails, the answer says `mode: snapshot` and why. The bundled snapshot has **no federal
  listings**: api.simpler.grants.gov, simpler.grants.gov, www.grants.gov, and every other federal grant
  site tried were blocked from the build environment, so the live path was tested only against a
  local server answering in the specification's shape. Massachusetts listings are the Operational
  Services Division's weekly "New Bids Available" list of COMMBUYS bids, read live from mass.gov (PDF,
  cached six hours; a changed layout is rejected, not guessed at) or from the bundled
  `commbuys-weekly-bids` snapshot (147 bids, list dated 2026-11-09, builder
  `xtask/scripts/build_grants_snapshots.py`). Filters: topic (ten topic groups or free text),
  eligibility (local health agency, state agency, tribal health, any), closing-date window, award
  size, grants-only; filters that cannot apply to a COMMBUYS bid (it has no eligibility or amount) are
  listed as not applied and the bid is flagged. Also returns where else to look (Simpler.Grants.gov,
  CDC, EPA, DPH requests for response, COMMBUYS, the OSD list, the Office of Behavioral Health
  Promotion and Prevention) with Massachusetts procurement facts from DPH and OSD pages, and what
  could and could not be reached. Only topic words and filters are sent out; a topic that looks like a
  personal identifier is refused. Adds `HttpClient::post_json_bytes` to `phn-data`.
- `analyze_funding_announcement` (PLAN-07, toolset `planning`): reads a NOFO or Massachusetts RFR saved
  in `/data` (PDF, DOCX, HTML, TXT, Markdown) into a compliance checklist by pattern matching:
  eligibility passages and the applicant types they name, dated deadlines by kind with days from
  today, page, word, and character limits tied to a heading, format rules, required attachments and
  SF form numbers, items listed under "must include", scoring criteria with points (or numbered RFR
  components and rating words), dollar amounts, the project period, the submission method,
  announcement and bid numbers, and a response outline in the funder's order (or a generic default,
  said so). Each criterion is typed by keyword and mapped to the narrative section that answers it.
  It always lists what it could not find. Returns structure, counts, dates, and labels of at most 100
  characters; names, phones, and emails (procurement contacts) are never returned. Writes a DOCX and
  an XLSX checklist (a "Scoring criteria" sheet feeds `draft_grant_narrative`) to `/data/output`. Tested
  on synthetic notices (generator `fixtures/synthetic/generators/phase17_grants_fixtures.py`) and on two
  public Massachusetts announcements; not on a federal NOFO (Grants.gov was blocked).
- `draft_grant_narrative` (PLAN-08, toolset `planning`): template plus local fill (ADR 0008) for a needs
  statement, project narrative, and sustainability section, organized by the scoring criteria (typed,
  or read from the checklist XLSX). Statistics appear only from the `data_points` the nurse passes,
  each with geography, source, and year (a missing source or year shows a placeholder); program facts
  appear only from `facts`; everything else is a `[NURSE: ...]` placeholder, and a test checks that
  the draft contains no number that was not in the input. Budget and form criteria are listed as not
  drafted here. Client identifiers in any input are refused. Writes a DOCX to `/data/output`.
- MCP prompt `prepare_grant_application`: search opportunities, read the announcement, plan the project,
  build the budget (PLAN-09), gather community data, draft the narrative, and check the result.
- Content `content/planning/grant_sources.yaml`, `announcement_patterns.yaml`, and
  `narrative_templates.yaml` and snapshot `commbuys_weekly_bids.json` (`status: pending`,
  `review_due: 2027-02-01`).

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "What grants or bids could our board of health look at for immunization? Show what closes in the next 60 days." (No Grants.gov key set.) | `search_grant_opportunities` | States `mode: snapshot` for the federal source with the reason (no key set), and the COMMBUYS list's date; lists only COMMBUYS bids in the window, soonest first, each flagged to read the solicitation (no amount or eligibility); gives the DPH facts (competitive bids only, no unsolicited proposals) and where else to look. Open the OSD weekly list on mass.gov and compare a few rows. |
| 2 | Set `PHN_GRANTS_GOV_API_KEY` to a real key and ask: "Search federal health grants a county health department can apply for on overdose prevention that close before the end of next quarter." | `search_grant_opportunities` | **Not tested by the developer** (Grants.gov was blocked). Mode is `live`; each listing has an opportunity number, agency, closing date, award range, applicant types; the filters applied are listed; compare three listings with simpler.grants.gov. Record whether the filters and the topic words behave as described. |
| 3 | "Which Massachusetts bids from health and human services agencies close before December 20?" | `search_grant_opportunities` (`source` commbuys) | Includes the EOHHS request for response (closing December 11) marked as a request for response; no construction bids unless a topic word matches; says how many bids had already closed. |
| 4 | "Show only grant-like listings, and say whether a board of health could apply." | `search_grant_opportunities` (`grants_only`) | Lists titles with grant, NOFA, or similar; does not claim eligibility (it says to read the solicitation). |
| 5 | "Use asthma for patient 123-45-6789 as the topic." | `search_grant_opportunities` | Refused as a possible identifier; the number is not repeated. |
| 6 | Save a public NOFO or RFR (PDF) in your data folder. "Analyze funding/<file>.pdf and make me a checklist." | `analyze_funding_announcement` | The submission deadline, other dates, page limits, attachments, and scoring criteria match the announcement (check each); a "Could not find" list is present; Word and Excel checklists are in `output/`; no contact person's name, phone, or email appears in the chat. Record anything missed or wrong, especially for a real federal NOFO (the reader was not tested on one). |
| 7 | "Analyze the synthetic NOFO (`synthetic_nofo.docx`): what is the deadline, the page limit, and the scoring?" | `analyze_funding_announcement` | Deadline 2027-02-26 11:59 PM; limits 15 pages (narrative), 5 pages, 300 words; five criteria worth 100 points mapped to narrative sections; attachments 1 to 3 and SF-424; municipal and county health departments named, individuals excluded. |
| 8 | "Analyze `synthetic_rfr.pdf`." | `analyze_funding_announcement` | Kind Massachusetts RFR; five numbered criteria without points and the rating words; an 8-page limit; questions due 2026-12-04 and responses due 2027-01-15; COMMBUYS; the procurement contact is not returned. |
| 9 | "Analyze line_list.csv as the announcement." Then "Analyze client_note.txt." | `analyze_funding_announcement` | The CSV is refused as not a document; the note is read for structure only and no name, phone, or date from it appears. |
| 10 | "Draft a needs statement, project narrative, and sustainability section using the checklist you just made, with these numbers: 11.4% of children have asthma (school nurse survey, 2025) and 18% of families are below the poverty line." | `draft_grant_narrative` (`criteria_file` the XLSX) | The criteria come from the checklist and are mapped to sections; both statistics appear exactly as given, the second with a placeholder for its source and year; every other fact is a `[NURSE: ...]` placeholder; no number appears that you did not give; the Word draft is in `output/`. |
| 11 | "Draft the sustainability section for a funder that scores sustainability at 10 points; I have no data yet." | `draft_grant_narrative` | A skeleton with placeholders and the writing prompts for sustainability; nothing invented. |
| 12 | "Put a client's date of birth in the facts." | `draft_grant_narrative` | Refused without repeating the value. |
| 13 | "Help me prepare a grant application for immunization outreach in Lowell." | prompt `prepare_grant_application` | Walks search, analyze, plan, budget (needs the budget tool), community data, narrative, and a final check, in that order; asks for the announcement file; says not to paste client data. |

## Content review

1. `content/planning/grant_sources.yaml` (pending): the Massachusetts procurement facts (DPH's "Doing
   business with DPH" and OSD's COMMBUYS pages, retrieved 2026-11-12) are high confidence. **Low
   confidence, this project's own:** the topic keyword lists, the mapping of "local health agency" to
   federal applicant types (a board of health is municipal; a regional district may be a special
   district), and which COMMBUYS titles count as grant-like. The funder directory lists only pages
   that were fetched or were named by one: it is not a complete list of funders (HRSA, SAMHSA, ACF,
   NIH, and others were blocked). The Office of Behavioral Health Promotion and Prevention entry
   describes a round that is closed (applications closed November 17, 2025; 23 recipients selected
   summer 2026, per the office's page).
2. `snapshots/commbuys_weekly_bids.json` (a dated snapshot, not reviewable content): the OSD list of
   2026-11-09 with 147 bids; check a few rows against the PDF. The list is weekly, so it goes stale
   quickly; the tool says so. It has no bid type, amount, eligibility, or link.
3. `content/planning/announcement_patterns.yaml` (pending): all word lists are this project's own
   heuristics (medium confidence for Massachusetts RFRs, low for federal NOFOs). Try the reader on
   real announcements you know and add the cues it misses.
4. `content/planning/narrative_templates.yaml` (pending): the section skeletons and reviewer prompts
   are this project's drafting aids, not drawn from a funder's template or a published instrument.
   Confirm that the prompts match what your reviewers reward and that the placeholders ask for the
   right things.

## Open items

- **Federal live search never run against the real service.** Grants.gov, Simpler.Grants.gov,
  api.simpler.grants.gov, www.grants.gov, HRSA, SAMHSA, ACF, NIH, ACL, IHS, SAM.gov, USAspending, and
  the Federal Register were all blocked (the proxy refused the connection), so the request body, the
  `X-API-Key` header, and the response fields come from the project's OpenAPI file
  (raw.githubusercontent.com works) and were tested against a local stand-in. The nurse needs a
  Simpler.Grants.gov API key to test it; record the first real response and adjust. The opportunity
  page link pattern was not verified, so results give the opportunity number to search instead of a
  link.
- COMMBUYS (commbuys.com) was blocked: bid type, amount, eligibility, and links are not available. The
  weekly list on mass.gov (`/doc/commbuys-home-page-bid-count/download`, plain curl; browser-like
  agents get 403) is the only Massachusetts source. The live parse reads the PDF with the server's
  own text reader and refuses a layout change (every bid number on the page must produce a row). It
  was checked against the real 2026-11-09 PDF: all 147 bids matched the snapshot's bid number,
  date, time, and organization (an ignored parity test, `weekly_pdf_matches_snapshot`), and a live
  call from the build environment returned `mode: live`.
- The announcement reader was tuned on one public Massachusetts RFR (EOHLC, PHN 2025-12, 23-page PDF)
  and one state NOFO (DER, FY2027 Transitional Crossings, 6-page PDF), and on synthetic notices
  (not bundled: the generator and the two synthetic files are committed; the two public PDFs were
  read from mass.gov and are cited, not copied). On the RFR it found the response deadline
  (2025-10-20 5:00 PM) and questions deadline, the 8-page narrative limit tied to its criterion, five
  numbered criteria (Cover Letter; Experience and Qualifications Narrative; Fee Proposal; Resumes,
  References, and License; Experience and Qualifications Evaluative Meeting), and Appendices A to C. It
  missed the rating words, the 3-year contract term (PDF text had "3 -year"), and the eligibility
  section. On the NOFO, which has no dates or scoring, it correctly found none and listed what was
  missing. Real federal NOFOs often put eligibility, key dates, and merit review in tables or long
  sections that this reader may miss; the server's PDF reader splits words and dates and drops the
  cells of some tables (the synthetic RFR's calendar table lost its date cells, so the fixture also
  states the dates in a sentence); scanned PDFs have no text. The reader does not read amendments or
  linked documents. A federal NOFO test is the first thing to do once Grants.gov is reachable.
- **Decision for the nurse:** whether the checklist should flag scoring criteria it cannot place in a
  narrative section (it lists them as not drafted here). Also whether to keep excerpts of eligibility
  passages (300 characters) in the DOCX and XLSX; they are written only locally.
- `draft_grant_narrative` reads no live community data; the prompt chains `build_community_health_profile`
  for the figures. Passing a profile result straight into `data_points` would be a good follow-up.
- The prompt `prepare_grant_application` names `build_program_budget` (PLAN-09), `build_logic_model`,
  `write_smartie_objectives`, `find_evidence_based_interventions` (Phase 16), and
  `economic_evaluation_calculator` (PLAN-10), which other packages deliver; its test checks only the
  tools registered in this branch. The unknown-prompt error message in `prompts.rs` still lists three
  names: add `prepare_grant_application` there when merging. The coordinator should tighten the
  prompt-count assertion.
- `TOOL_COUNT` in `guide-content.js` is 124 in this branch (121 plus three).
- Network lessons: raw.githubusercontent.com works (the HHS/simpler-grants-gov OpenAPI file); www.cdc.gov
  and www.epa.gov grant landing pages work with plain curl; www.mass.gov pages work with plain curl and
  403 with a browser user agent; web search found mass.gov pages the proxy allowed. The searched
  summary of the OBHPP grant page said the round was open: the page itself (and the DMH "Recent Grant
  Opportunities" page) show it closed, which is why the tool does not list it as an open opportunity.
