# Acceptance script: Phase 13 (emergency preparedness II: ICS, plans, and exercises)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `epr`, `comms`, and `core` toolsets enabled (for example
`PHN_PROFILE=preparedness`). `staff_roster.csv` holds fictional staff with phone numbers and
emails: those must appear only in the documents written to `output/`, never in the chat. Record
pass/fail and notes. Pilot 3 is a tabletop exercise designed with `design_hseep_exercise`.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
<!-- Phase 13 EPR-01 (ICS) rows go above this line. -->
<!-- Phase 13 EPR-07 (COOP) rows go above this line. -->
<!-- Phase 13 EPR-08/EPR-09 (HSEEP) rows go above this line. -->
| CERC-1 | "We had a measles case at the Pollard Library on Tuesday from 2 to 5 p.m. Draft the initial message for Lowell: one case confirmed, we don't know yet if anyone else is sick, we're contacting visitors, next update by noon tomorrow." | `crisis_risk_communication_builder` | Message map with 3 key messages (≤ 27 words), each with 3 cited supporting facts; holding statement with what we know, don't know yet, are doing, and the next update; FAQs; spokesperson bridges and questions; language plan lists Lowell's priority languages (Spanish, Khmer, Portuguese, ...) with translator briefs. Works offline. Read the messages aloud: are they accurate and right for your town? |
| CERC-2 | "Use staff_roster.csv and write the packet to Word, in Spanish and Portuguese." (First save an approved Spanish message as `templates/spanish/cerc_measles_exposure.md`.) | `crisis_risk_communication_builder` | Spanish uses your translation (filled with the place and times); Portuguese gets a translator brief marked DRAFT TRANSLATION; the packet in `output/cerc_measles_exposure/` shows the PIO's phone and email, which must NOT appear in the chat. |
| CERC-3 | "Check this boil water message: 'A devastating contamination happened. Never drink the water. It is completely safe to shower.'" | `crisis_risk_communication_builder` | Draft checks flag loaded words, absolutes, and missing empathy and next-update information. |
| CERC-4 | "Give me the maintenance and resolution messages for a coastal storm with power outages." | `crisis_risk_communication_builder` | Phase tasks for both phases with the hazard notes; generator and CO messages; other protective messages from the hazard alert content. |
<!-- Phase 13 EPR-10 (CERC) rows go above this line. -->

## Content review (the subject matter expert)

All Phase 13 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

<!-- Phase 13 EPR-01 (ICS) review items go above this line. -->
<!-- Phase 13 EPR-07 (COOP) review items go above this line. -->
<!-- Phase 13 EPR-08/EPR-09 (HSEEP) review items go above this line. -->
- `content/epr/cerc.yaml` (EPR-10): the phase tasks and message-map rules against the CERC chapters
  and the ATSDR and EPA message mapping guides; every pre-scripted message, FAQ, and phase note
  against current DPH and CDC guidance and your agency's plans. Items needing the most attention:
  the mass dispensing set (who may pick up medicine, forms, and dosing by weight are low
  confidence and depend on your POD plan); the boil water set (MassDEP's boil order pages could not
  be read; coordinate with water suppliers); the measles FAQs on infants and work or school
  exclusion (medium); the respiratory set's vaccine wording (recommendations differ among DPH,
  professional societies, and CDC).
<!-- Phase 13 EPR-10 (CERC) review items go above this line. -->

## Decisions for the nurse

- CERC (EPR-10): which hazards to add to the pre-scripted library (for example winter storm,
  harmful algal bloom, foodborne outbreak, or a cyberattack on the department); whether the
  default reading grade target for crisis messages should be lower than 8 (EPA suggests 6th to
  8th grade); and who in your agency approves translated pre-scripted messages before they are
  saved to `/data/templates/<language>/cerc_<id>.md`.
<!-- Phase 13 decisions go above this line. -->
