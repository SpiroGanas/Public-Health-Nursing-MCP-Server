# Acceptance script: Phase 13 (emergency preparedness II: ICS, plans, and exercises)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `epr`, `comms`, and `core` toolsets enabled (for example
`PHN_PROFILE=preparedness`). `staff_roster.csv` holds fictional staff with phone numbers and
emails: those must appear only in the documents written to `output/`, never in the chat. Record
pass/fail and notes. Pilot 3 is a tabletop exercise designed with `design_hseep_exercise`.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Make an Incident Action Plan for the Maple Grove School measles exposure for tomorrow 0700 to 1900. I am the Planning Section Chief. Use staff_roster.csv for the organization and contacts. Objectives: notify all exposed students and staff by noon; offer MMR at a 2 p.m. clinic." | `generate_ics_forms` | IAP folder in `output/` with ICS 202, 203, 204, 205, 205A, 206, 207, 208 and `Incident_Action_Plan.docx`. The chat shows only counts for the roster (12 staff) and the blocks still blank (for example the 206 hospitals); no staff names or phone numbers from the file. Open the IAP: block numbers and labels match FEMA's forms; the 205A lists contacts and is marked "not for public release"; measles suggestions (N95, documented immunity) are offered with sources. |
| 2 | "We need 200 cases of bottled water at the senior center cooling site, urgent. Write the ICS 213RR, and give me blank ICS 214 activity logs." | `generate_ics_forms` | A filled 213RR (blocks 1-9; Logistics and Finance blocks left for them) and blank 214s with FEMA's block instructions in `output/…/blank/`. |
| 3 | "Write an ICS 215A and 208 for a coastal storm with power outages, using your suggestions." | `generate_ics_forms` | Hazards include carbon monoxide (generators more than 20 feet from windows, doors, and vents; CDC) and flood water (CDC), each marked "suggested; review"; low-confidence lines are labeled. |
<!-- Phase 13 EPR-01 (ICS) rows go above this line. -->
<!-- Phase 13 EPR-07 (COOP) rows go above this line. -->
<!-- Phase 13 EPR-08/EPR-09 (HSEEP) rows go above this line. -->
<!-- Phase 13 EPR-10 (CERC) rows go above this line. -->

## Content review (the subject matter expert)

All Phase 13 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

1. `content/epr/ics_forms.yaml`: form titles, versions, block numbers, and labels were checked
   against FEMA's ICS forms (v3/v3.1); confirm they match the forms your HMCC and MEMA use. The
   ICS 213RR block instructions are this project's summary (FEMA's PDF has none). Every incident
   suggestion (objectives, command emphasis, safety messages, 215A hazards and mitigations) is a
   draft; lines citing CDC or NIOSH restate those pages, the rest are tool suggestions (medium or
   low confidence).
<!-- Phase 13 EPR-01 (ICS) review items go above this line. -->
<!-- Phase 13 EPR-07 (COOP) review items go above this line. -->
<!-- Phase 13 EPR-08/EPR-09 (HSEEP) review items go above this line. -->
<!-- Phase 13 EPR-10 (CERC) review items go above this line. -->

## Decisions for the nurse

- ICS forms (EPR-01): are FEMA-layout adaptations acceptable, or must the agency submit FEMA's
  fillable PDFs (the tool cannot fill those)? Should the ICS 207 stay in the default IAP (FEMA
  calls it a wall chart, not an IAP form, but the ICS 202 lists it)? Which incident types need
  suggestions beyond the seven drafted (for example water contamination, cyberattack)?
<!-- Phase 13 decisions go above this line. -->
