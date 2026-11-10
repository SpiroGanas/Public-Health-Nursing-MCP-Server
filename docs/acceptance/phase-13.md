# Acceptance script: Phase 13 (emergency preparedness II: ICS, plans, and exercises)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `epr`, `comms`, and `core` toolsets enabled (for example
`PHN_PROFILE=preparedness`). `staff_roster.csv` holds fictional staff with phone numbers and
emails: those must appear only in the documents written to `output/`, never in the chat. Record
pass/fail and notes. Pilot 3 is a tabletop exercise designed with `design_hseep_exercise`.

Prompt 16 may also suggest `plan_vaccination_clinic`, which is in the `vax` toolset and is not
part of the `preparedness` profile. To include it, start the server with
`PHN_TOOLSETS=epr,env,epi,equity,comms,cd,vax` instead of the profile.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Make an Incident Action Plan for the Maple Grove School measles exposure for tomorrow 0700 to 1900. I am the Planning Section Chief. Use staff_roster.csv for the organization and contacts. Objectives: notify all exposed students and staff by noon; offer MMR at a 2 p.m. clinic." | `generate_ics_forms` | IAP folder in `output/` with ICS 202, 203, 204, 205, 205A, 206, 207, 208 and `Incident_Action_Plan.docx`. The chat shows only counts for the roster (12 staff) and the blocks still blank (for example the 206 hospitals); no staff names or phone numbers from the file. Open the IAP: block numbers and labels match FEMA's forms; the 205A lists contacts and is marked "not for public release"; measles suggestions (N95, documented immunity) are offered with sources. |
| 2 | "We need 200 cases of bottled water at the senior center cooling site, urgent. Write the ICS 213RR, and give me blank ICS 214 activity logs." | `generate_ics_forms` | A filled 213RR (blocks 1-9; Logistics and Finance blocks left for them) and blank 214s with FEMA's block instructions in `output/…/blank/`. |
| 3 | "Write an ICS 215A and 208 for a coastal storm with power outages, using your suggestions." | `generate_ics_forms` | Hazards include carbon monoxide (generators more than 20 feet from windows, doors, and vents; CDC) and flood water (CDC), each marked "suggested; review"; low-confidence lines are labeled. |
| 4 | "Draft a COOP plan for the Wilmington Board of Health dated today. We have 6 staff. Our alternate site is the library meeting room (MOU with the trustees, ready in 8 hours) and staff can telework. Use staff_roster.csv for contacts." | `continuity_of_operations_planner` | 19 essential functions in four tiers with authority (105 CMR 300.110/300.160, c. 111 sections 30, 95, 122, 127A, c. 114 section 45); five delegations including the c. 111 section 30 emergency agent; staffing check says about 3 staff remain at 40% absence; plan elements show facilities and human capital `provided`, devolution `missing`; first-year TT&E dates; Word plan and Excel workbook in `output/`. No roster names, phone numbers, or emails in the chat. Open the plan: the succession table and call-down annex show the roster's names and contacts. |
| 5 | "Make the Incident Commander succession three deep: Operations Section Chief, Planning Section Chief, Health Agent. Move routine immunization clinics to Tier 2, add animal bite follow-up as Tier 1, drop seasonal permits, and name the neighboring board of health as our devolution partner." | `continuity_of_operations_planner` | Succession meets three-deep; immunization shows `seed_changed` in Tier 2; the new function is `your_input` in Tier 1; seasonal permits gone; devolution `partial` until a devolution site is added. Check the tiers and recovery times against your own priorities. |
| 6 | "Design a measles tabletop exercise for Amherst on March 17, 2027, at Town Hall, 9 a.m., 3.5 hours. Participants: Incident Commander, Public Information Officer, Medical Unit Leader as evaluator, and the school nurse leader. Use staff_roster.csv." | `design_hseep_exercise` | Five suggested SMART objectives, each with PHEP and FEMA core capabilities; three modules (notification, prophylaxis and exclusion, second-generation cases) with injects and questions by functional group; schedule from 8:30 registration to the hotwash; planning meetings dated back from March 17; SitMan, Facilitator Guide, EEGs, feedback form, and sign-in sheet in `output/exercise_.../`. No phone numbers or emails from the roster in the chat; open the SitMan and EEGs and confirm they are there. **Pilot 3: run this tabletop with your planning team and note what to change.** |
| 7 | "Plan a tabletop on a coastal storm with a multi-day power outage for Plymouth" and "...on extreme heat for Lowell" | `design_hseep_exercise` | The storm and heat scenarios fill in the town; questions cover emPOWER outreach, CMIST sheltering, CO messaging, cooling centers, and heat surveillance; related tools listed. |
| 8 | "Check these objectives: 'Improve communication' and 'Within 2 hours of notification, discuss how the nurse will begin case investigation in accordance with the DPH guide.'" | `design_hseep_exercise` | The first fails S, M, R, and T with suggestions; the second passes and maps to PHEP 13 with FEMA capabilities added (labeled as this project's mapping). |
| 9 | "Make it a functional exercise on extreme heat with an MSEL." | `design_hseep_exercise` | Exercise Plan (no scenario details), MSEL workbook with HSEEP's columns and one row per inject, controller/evaluator debrief in the schedule. |
| 10 | "Write the after-action report for the measles tabletop: [objectives with ratings, two strengths, two areas for improvement, corrective actions with owners and dates]. Add the feedback forms in exercise_feedback.csv." | `build_after_action_report` | Table 1 ratings; improvement plan numbered 1.1, 2.1; all checks pass; feedback averages for 8 statements from 14 forms; no names, emails, or comments from the file in the chat; AAR DOCX and improvement plan XLSX in `output/aar_.../`. Then remove an owner and a date and confirm the checks catch it. |
| 11 | "Add this finding: 'Nurse Maria Garcia could not reach the shelter manager at 413-555-0199.'" | `build_after_action_report` | The non-attribution and client-identifier checks fail and ask you to describe positions, not people. |
| 12 | "We had a measles case at the Pollard Library on Tuesday from 2 to 5 p.m. Draft the initial message for Lowell: one case confirmed, we don't know yet if anyone else is sick, we're contacting visitors, next update by noon tomorrow." | `crisis_risk_communication_builder` | Message map with 3 key messages (≤ 27 words), each with 3 cited supporting facts; holding statement with what we know, don't know yet, are doing, and the next update; FAQs; spokesperson bridges and questions; language plan lists Lowell's priority languages (Spanish, Khmer, Portuguese, ...) with translator briefs. Works offline. Read the messages aloud: are they accurate and right for your town? |
| 13 | "Use staff_roster.csv and write the packet to Word, in Spanish and Portuguese." (First save an approved Spanish message as `templates/spanish/cerc_measles_exposure.md`.) | `crisis_risk_communication_builder` | Spanish uses your translation (filled with the place and times); Portuguese gets a translator brief marked DRAFT TRANSLATION; the packet in `output/cerc_measles_exposure/` shows the PIO's phone and email, which must NOT appear in the chat. |
| 14 | "Check this boil water message: 'A devastating contamination happened. Never drink the water. It is completely safe to shower.'" | `crisis_risk_communication_builder` | Draft checks flag loaded words, absolutes, and missing empathy and next-update information. |
| 15 | "Give me the maintenance and resolution messages for a coastal storm with power outages." | `crisis_risk_communication_builder` | Phase tasks for both phases with the hazard notes; generator and CO messages; other protective messages from the hazard alert content. |
| 16 | Choose the "Plan a tabletop exercise" prompt with scenario "measles outbreak", area "Amherst", and roster "staff_roster.csv". Follow it through design, materials, and the after-action report. | prompt `plan_tabletop_exercise` | Claude works through the five steps in order and calls `design_hseep_exercise`, `generate_ics_forms`, `crisis_risk_communication_builder`, and `build_after_action_report` (and the context tools in step 1); the scenario is labeled fictional; no phone numbers or emails appear in the chat. |

## Content review (the subject matter expert)

All Phase 13 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

1. `content/epr/ics_forms.yaml`: form titles, versions, block numbers, and labels were checked
   against FEMA's ICS forms (v3/v3.1); confirm they match the forms your HMCC and MEMA use. The
   ICS 213RR block instructions are this project's summary (FEMA's PDF has none). Every incident
   suggestion (objectives, command emphasis, safety messages, 215A hazards and mitigations) is a
   draft; lines citing CDC or NIOSH restate those pages, the rest are tool suggestions (medium or
   low confidence).
2. `content/epr/continuity.yaml` (EPR-07): the 19 board of health essential functions, their
   tiers, recovery times, and minimum staff (tool defaults); the authority cited for each
   (statutes read on malegislature.gov; 105 CMR 300.110/300.160 from this project's reportable
   conditions content); the five delegation templates (town counsel to confirm); the default
   succession positions, essential records, communications, and TT&E frequencies (tool defaults);
   and the FEMA element checklists and phases (FEMA's Continuity Guidance Circular could not be
   retrieved: medium confidence). Low-confidence functions: wellness checks for high-risk
   clients, routine nursing programs, routine immunization, community programs.
3. `content/epr/capabilities.yaml`: the PHEP capability definitions and functions (CDC 2018
   standards), the FEMA core capability names (National Preparedness Goal names used where FEMA's
   EEG files differ), and especially **the PHEP to FEMA mapping, which is this project's own**
   (no official crosswalk was found). Confirm with DPH or your HMCC what your agency reports.
4. `content/epr/hseep.yaml`: exercise types, documents, planning meetings, SMART guidance,
   ratings, feedback-form statements, MSEL columns, and AAR/IP rules (transcribed from HSEEP 2020
   and its templates); **the planning-meeting weeks, schedule lengths, hotwash agenda, and the
   30/45/60-day after-action dates are tool defaults**.
5. `content/epr/exercise_scenarios.yaml`: **all three Pilot 3 scenarios are this project's
   fictional drafts** (injects, numbers, key issues, questions, and suggested objectives). Check
   them against your plans and DPH guidance, and confirm the measles facts (infectious period,
   72-hour MMR and 6-day immune globulin windows, days 5-21 exclusion).
6. `content/epr/cerc.yaml` (EPR-10): the phase tasks and message-map rules against the CERC
   chapters and the ATSDR and EPA message mapping guides; every pre-scripted message, FAQ, and
   phase note against current DPH and CDC guidance and your agency's plans. Items needing the
   most attention: the mass dispensing set (who may pick up medicine, forms, and dosing by weight
   are low confidence and depend on your POD plan); the boil water set (MassDEP's boil order
   pages could not be read; coordinate with water suppliers); the measles FAQs on infants and
   work or school exclusion (medium); the respiratory set's vaccine wording (recommendations
   differ among DPH, professional societies, and CDC).

## Decisions for the nurse

- ICS forms (EPR-01): are FEMA-layout adaptations acceptable, or must the agency submit FEMA's
  fillable PDFs (the tool cannot fill those)? Should the ICS 207 stay in the default IAP (FEMA
  calls it a wall chart, not an IAP form, but the ICS 202 lists it)? Which incident types need
  suggestions beyond the seven drafted (for example water contamination, cyberattack)?
- COOP (EPR-07): the tier and recovery time for each essential function (especially burial
  permits, TB directly observed therapy, and food and housing emergencies, set to Tier 2); whether
  to keep the 40% pandemic absenteeism assumption (2006 federal plan) or use your own; whether
  the agency wants a devolution agreement with a neighboring board of health or health district;
  and who in the agency owns the yearly COOP review.
- CERC (EPR-10): which hazards to add to the pre-scripted library (for example winter storm,
  harmful algal bloom, foodborne outbreak, or a cyberattack on the department); whether the
  default reading grade target for crisis messages should be lower than 8 (EPA suggests 6th to
  8th grade); and who in your agency approves translated pre-scripted messages before they are
  saved to `/data/templates/<language>/cerc_<id>.md`.
- HSEEP (EPR-08, EPR-09): keep the PHEP to FEMA mapping or replace it with a crosswalk from DPH;
  the planning timeline and after-action due dates (tool defaults) versus your grant deadlines;
  whether EEGs should be one per objective and PHEP capability (current) or one per FEMA core
  capability (FEMA's templates); which scenario Pilot 3 uses and who reviews it.
