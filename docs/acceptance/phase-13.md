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
| H1 | "Design a measles tabletop exercise for Amherst on March 17, 2027, at Town Hall, 9 a.m., 3.5 hours. Participants: Incident Commander, Public Information Officer, Medical Unit Leader as evaluator, and the school nurse leader. Use staff_roster.csv." | `design_hseep_exercise` | Five suggested SMART objectives, each with PHEP and FEMA core capabilities; three modules (notification, prophylaxis and exclusion, second-generation cases) with injects and questions by functional group; schedule from 8:30 registration to the hotwash; planning meetings dated back from March 17; SitMan, Facilitator Guide, EEGs, feedback form, and sign-in sheet in `output/exercise_.../`. No phone numbers or emails from the roster in the chat; open the SitMan and EEGs and confirm they are there. **Pilot 3: run this tabletop with your planning team and note what to change.** |
| H2 | "Plan a tabletop on a coastal storm with a multi-day power outage for Plymouth" and "...on extreme heat for Lowell" | `design_hseep_exercise` | The storm and heat scenarios fill in the town; questions cover emPOWER outreach, CMIST sheltering, CO messaging, cooling centers, and heat surveillance; related tools listed. |
| H3 | "Check these objectives: 'Improve communication' and 'Within 2 hours of notification, discuss how the nurse will begin case investigation in accordance with the DPH guide.'" | `design_hseep_exercise` | The first fails S, M, R, and T with suggestions; the second passes and maps to PHEP 13 with FEMA capabilities added (labeled as this project's mapping). |
| H4 | "Make it a functional exercise on extreme heat with an MSEL." | `design_hseep_exercise` | Exercise Plan (no scenario details), MSEL workbook with HSEEP's columns and one row per inject, controller/evaluator debrief in the schedule. |
| H5 | "Write the after-action report for the measles tabletop: [objectives with ratings, two strengths, two areas for improvement, corrective actions with owners and dates]. Add the feedback forms in exercise_feedback.csv." | `build_after_action_report` | Table 1 ratings; improvement plan numbered 1.1, 2.1; all checks pass; feedback averages for 8 statements from 14 forms; no names, emails, or comments from the file in the chat; AAR DOCX and improvement plan XLSX in `output/aar_.../`. Then remove an owner and a date and confirm the checks catch it. |
| H6 | "Add this finding: 'Nurse Maria Garcia could not reach the shelter manager at 413-555-0199.'" | `build_after_action_report` | The non-attribution and client-identifier checks fail and ask you to describe positions, not people. |
<!-- Phase 13 EPR-08/EPR-09 (HSEEP) rows go above this line. -->
<!-- Phase 13 EPR-10 (CERC) rows go above this line. -->

## Content review (the subject matter expert)

All Phase 13 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

<!-- Phase 13 EPR-01 (ICS) review items go above this line. -->
<!-- Phase 13 EPR-07 (COOP) review items go above this line. -->
- `content/epr/capabilities.yaml`: the PHEP capability definitions and functions (CDC 2018 standards), the FEMA core capability names (National Preparedness Goal names used where FEMA's EEG files differ), and especially **the PHEP to FEMA mapping, which is this project's own** (no official crosswalk was found). Confirm with DPH or your HMCC what your agency reports.
- `content/epr/hseep.yaml`: exercise types, documents, planning meetings, SMART guidance, ratings, feedback-form statements, MSEL columns, and AAR/IP rules (transcribed from HSEEP 2020 and its templates); **the planning-meeting weeks, schedule lengths, hotwash agenda, and the 30/45/60-day after-action dates are tool defaults**.
- `content/epr/exercise_scenarios.yaml`: **all three Pilot 3 scenarios are this project's fictional drafts** (injects, numbers, key issues, questions, and suggested objectives). Check them against your plans and DPH guidance, and confirm the measles facts (infectious period, 72-hour MMR and 6-day immune globulin windows, days 5-21 exclusion).
<!-- Phase 13 EPR-08/EPR-09 (HSEEP) review items go above this line. -->
<!-- Phase 13 EPR-10 (CERC) review items go above this line. -->

## Decisions for the nurse

- HSEEP tools: keep the PHEP to FEMA mapping or replace it with a crosswalk from DPH; the planning timeline and after-action due dates (tool defaults) versus your grant deadlines; whether EEGs should be one per objective and PHEP capability (current) or one per FEMA core capability (FEMA's templates); which scenario Pilot 3 uses and who reviews it.
<!-- Phase 13 decisions go above this line. -->
