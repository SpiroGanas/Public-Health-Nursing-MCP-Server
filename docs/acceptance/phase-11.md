# Acceptance script: Phase 11 (substance use, crisis resources, and health communication)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `bh` and `comms` toolsets enabled (for example `PHN_PROFILE=school_nurse`,
or `PHN_TOOLSETS=bh,comms,equity`). Record pass/fail and notes. The rules, formulas, and privacy
behavior are also checked automatically (`crates/phn-server/tests/phase11_tools.rs`, the leak
test in `crates/phn-server/tests/mcp_integration.rs`, the readability golden tests in
`crates/phn-tools/tests/readability_golden.rs`, and the unit tests in
`crates/phn-tools/src/bh/` and `crates/phn-tools/src/comms/`); this script checks that the
answers are correct for Massachusetts practice and useful.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Check the reading level of this flu flyer and tell me what to fix: [paste a real flyer]." | `check_readability` | Four grades and the median against grade 8; long sentences, CDC Everyday Words alternatives, passive voice, and acronyms flagged; Clear Communication Index items with the tool's suggestions; compare the grade with Word or another tool (within about one grade). |
| 2 | "Score it with the Clear Communication Index. My answers: 1 yes, 2 yes, 3 no, ... (all 20, plus C and D)." | `check_readability` | Score matches a hand calculation on CDC's score sheet; 90 or higher passes; items scoring 0 listed. |
| 3 | "How many opioid overdose deaths has our town had, and how does its rate compare with the state?" | `get_overdose_data` | DPH's 2015–2022 counts by residence; 2018–2022 rate with a 95% CI, or "fewer than 5 deaths: no rate"; CDC county counts for the latest 12 months with the change; link to the BSAS dashboard for newer data. Spot-check two towns against DPH's June 2023 PDF. |
| 4 | "What is the latest CDC count of fentanyl deaths in Massachusetts compared with the U.S.?" | `get_overdose_data` | Predicted 12 month-ending counts with the period and percent change; drug-class table; notes that deaths are counted by state of occurrence. Compare with CDC's dashboard. |
| 5 | "What share of Massachusetts high school students felt sad or hopeless, by sex and race?" | `get_youth_risk_behavior_data` | MYHS 2025 table values with 95% CIs (overall 27.7%, female 36.2%); differences flagged only when CIs do not overlap; CDC MA vs U.S. comparison; 988 line. Check three numbers against the 2025 MYHS report. |
| 6 | "How common is vaping among middle school students, by grade?" | `get_youth_risk_behavior_data` | Middle school MYHS indicators only, by grade; no CDC comparison (high school only). |
| 7 | "Where can a family in Pittsfield get Narcan, and is there a syringe services program? Give me a teaching handout." | `find_naloxone_and_harm_reduction` | Pharmacy counter under the statewide standing order (no MassHealth co-pay), programs map link, Pittsfield approved for an SSP, teaching script steps, Word handout in `output/` that passes `check_document_accessibility`. Read the script for accuracy and tone. |
| 8 | "Find buprenorphine treatment within 10 miles of Fall River that takes MassHealth and has Spanish." | `find_substance_use_treatment` | FindTreatment.gov results nearest first with phones and MOUD; live mode (or the bundled copy with a caveat if offline); Substance Use Helpline. Call one listing to confirm. |
| 9 | "A student died over the weekend and the cause isn't confirmed. What do we do today, and what do we tell students and parents?" | `plan_school_postvention` | First-24-hours steps with toolkit pages; student and parent templates for an unconfirmed cause; 988; decision-support notice. Review the templates' tone with the school crisis team. |
| 10 | "Check this draft letter for safe messaging: [paste a draft that says 'committed suicide' and gives a cause]." | `plan_school_postvention` | Outdated language and simple cause flagged by sentence number; missing help resources flagged; no method words repeated. |
| 11 | "Fill the parent letter and student statement from postvention_details.csv." | `plan_school_postvention` | Word files in `output/`; the response shows file names and counts only, never the student's or staff names. Open the files. |
| 12 | "Which languages should our flu flyer be translated into for Lawrence? Prepare a Spanish translator brief using glossary.csv." | `translate_health_material` | Language priorities from ACS; segments, glossary terms, and invariants (numbers, phones, links); the "DRAFT TRANSLATION - for review by a qualified translator" label; no translation produced by the tool. |
| 13 | "Here is the Spanish draft and a back-translation; what changed?" | `translate_health_material` | Changed numbers or phones, missing glossary terms, negation differences, and missing sentences are listed; a review worksheet in `output/` if asked. |
| 14 | "Plan a four-week flu campaign on Facebook and Instagram starting December 1 and write the first week of posts." | `create_social_media_campaign` | Calendar tied to National Influenza Vaccination Week (confirm the date with CDC); posts checked for length, alt text, CamelCase hashtags, and reading grade; unverified-limits caveat. |
| 15 | "Draft a press release for free flu clinics Saturday at Town Hall, then a spokesperson Q&A." | `draft_press_materials` | Contact block, date, "For immediate release", headline, 5 Ws, gaps in brackets; CERC checks; Q&A with CERC's questions; Word export. |

## Content review (the subject matter expert)

All Phase 11 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

1. `content/comms/readability.yaml`: the Clear Communication Index items and scoring rules
   against CDC's score sheet; the jargon list (CDC Everyday Words, 124 terms) and the
   `check_context` choices; the default target grade (8, a tool default; set your agency's
   standard); the Flesch interpretation bands (medium confidence).
2. `content/bh/overdose_data.yaml`: the 2024 findings and page numbers against DPH's March 2026
   report; confirm DPH's small-number rules (the tool gives no rate for fewer than 5 deaths and
   flags rates on fewer than 20 as unstable).
3. `content/bh/youth_survey.yaml`: survey descriptions and the 988 text; whether comparing
   groups with male, White, and straight/cisgender students is the comparison you want.
4. `content/bh/substance_use_resources.yaml`: the standing order formulations (July 13, 2026),
   pharmacy rules, legal protections, the list of towns with SSP approval, contacts, and every
   step of the naloxone teaching script (adapted from DPH's 2019 OEND competencies). Decide how
   to describe fentanyl test strips: their legal status under M.G.L. c. 94C, s. 1 could not be
   confirmed (low confidence).
5. `content/bh/postvention.yaml` and `content/comms/safe_messaging.yaml`: the step paraphrases
   and page numbers against *After a Suicide: A Toolkit for Schools*; the wording of every
   template; the safe-messaging word patterns (medium confidence). Decide whether to seek EDC's
   permission (permissions@edc.org) to adapt the toolkit's own sample statements instead.
6. `content/comms/translation.yaml`: the workflow and the draft label wording.
7. `content/comms/campaigns.yaml`: **verify every platform limit and every observance date**
   (both low confidence because their sources could not be reached), and the CERC checklist.

## Data checks

- Snapshots `ma-dph-opioid-deaths-town`, `cdc-vsrr-overdose-ma`, `ma-youth-health-survey`,
  `cdc-yrbs-mental-health-ma-us`, and `samhsa-findtreatment-ma` were built by the scripts in
  `xtask/scripts/` on 2026-11-08 and 2026-11-09; rebuild them when DPH or CDC publish newer data
  (the DPH youth survey is biennial; CDC's provisional counts update monthly).
