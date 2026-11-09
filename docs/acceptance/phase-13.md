# Acceptance script: Phase 13 (emergency preparedness II: ICS, plans, and exercises)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `epr`, `comms`, and `core` toolsets enabled (for example
`PHN_PROFILE=preparedness`). `staff_roster.csv` holds fictional staff with phone numbers and
emails: those must appear only in the documents written to `output/`, never in the chat. Record
pass/fail and notes. Pilot 3 is a tabletop exercise designed with `design_hseep_exercise`.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
<!-- Phase 13 EPR-01 (ICS) rows go above this line. -->
| C1 | "Draft a COOP plan for the Wilmington Board of Health dated today. We have 6 staff. Our alternate site is the library meeting room (MOU with the trustees, ready in 8 hours) and staff can telework. Use staff_roster.csv for contacts." | `continuity_of_operations_planner` | 19 essential functions in four tiers with authority (105 CMR 300.110/300.160, c. 111 sections 30, 95, 122, 127A, c. 114 section 45); five delegations including the c. 111 section 30 emergency agent; staffing check says about 3 staff remain at 40% absence; plan elements show facilities and human capital `provided`, devolution `missing`; first-year TT&E dates; Word plan and Excel workbook in `output/`. No roster names, phone numbers, or emails in the chat. Open the plan: the succession table and call-down annex show the roster's names and contacts. |
| C2 | "Make the Incident Commander succession three deep: Operations Section Chief, Planning Section Chief, Health Agent. Move routine immunization clinics to Tier 2, add animal bite follow-up as Tier 1, drop seasonal permits, and name the neighboring board of health as our devolution partner." | `continuity_of_operations_planner` | Succession meets three-deep; immunization shows `seed_changed` in Tier 2; the new function is `your_input` in Tier 1; seasonal permits gone; devolution `partial` until a devolution site is added. Check the tiers and recovery times against your own priorities. |
<!-- Phase 13 EPR-07 (COOP) rows go above this line. -->
<!-- Phase 13 EPR-08/EPR-09 (HSEEP) rows go above this line. -->
<!-- Phase 13 EPR-10 (CERC) rows go above this line. -->

## Content review (the subject matter expert)

All Phase 13 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

<!-- Phase 13 EPR-01 (ICS) review items go above this line. -->
- `content/epr/continuity.yaml` (EPR-07): the 19 board of health essential functions, their
  tiers, recovery times, and minimum staff (tool defaults); the authority cited for each (statutes
  read on malegislature.gov; 105 CMR 300.110/300.160 from this project's reportable conditions
  content); the five delegation templates (town counsel to confirm); the default succession
  positions, essential records, communications, and TT&E frequencies (tool defaults); and the
  FEMA element checklists and phases (FEMA's Continuity Guidance Circular could not be retrieved:
  medium confidence). Low-confidence functions: wellness checks for high-risk clients, routine
  nursing programs, routine immunization, community programs.
<!-- Phase 13 EPR-07 (COOP) review items go above this line. -->
<!-- Phase 13 EPR-08/EPR-09 (HSEEP) review items go above this line. -->
<!-- Phase 13 EPR-10 (CERC) review items go above this line. -->

## Decisions for the nurse

- COOP (EPR-07): the tier and recovery time for each essential function (especially burial
  permits, TB directly observed therapy, and food and housing emergencies, set to Tier 2); whether
  to keep the 40% pandemic absenteeism assumption (2006 federal plan) or use your own; whether
  the agency wants a devolution agreement with a neighboring board of health or health district;
  and who in the agency owns the yearly COOP review.
<!-- Phase 13 decisions go above this line. -->
