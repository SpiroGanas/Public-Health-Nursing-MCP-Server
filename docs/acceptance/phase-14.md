# Acceptance script: Phase 14 (environmental health and climate)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `env`, `chronic`, `equity`, and `core` toolsets enabled (for example
`PHN_TOOLSETS=env,chronic,equity,epi,core`). Record pass/fail and notes.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
<!-- Phase 14 Water (ENV-03, CHR-07) prompts. -->
<!-- End of Phase 14 water prompts. -->
<!-- Phase 14 EJ and climate (ENV-04, ENV-05) prompts. -->
<!-- End of Phase 14 ejclimate prompts. -->
<!-- Phase 14 Vectors and housing (ENV-06, ENV-07) prompts. -->
| 1 | "What is the EEE and West Nile virus risk in Middleborough right now, and what should we tell the schools?" | `vector_borne_disease_risk` | Both levels for Middleborough with the date DPH last updated them. Outside June to October (or offline) the answer says the season is over (or not started) and that the levels are NOT current risk; in season and online it says the levels are current. What the level means, residents' actions, and DPH's plan response match DPH's dashboard and the plan's Tables 1–2. School messages cite DPH's repellent guidance for school staff. Plymouth County tick-borne disease table with Massachusetts rates. |
| 2 | "Which towns in Plymouth County were at moderate or higher EEE risk at the end of the 2024 season, and how many human EEE cases were there?" | `vector_borne_disease_risk` | 2024 final levels (as of 2024-10-30); towns listed; human cases shown only as county counts (no ages, sexes, or towns). Compare two towns with DPH's 2024 workbook. |
| 3 | "How common are babesiosis and anaplasmosis in Barnstable County, and is it getting worse? Make a one-page notice for camps." | `vector_borne_disease_risk` | 2022–2025 counts and DPH rates for Barnstable with the state rate and change from the previous year; counts under 20 flagged; ED visit series; camp messages cite 105 CMR 430.159(B)(4) and 430.401(A); DOCX notice in `output/`. |
| 4 | "A family in Holyoke has roaches and a leak in the bathroom ceiling. What does the sanitary code require, who has to fix it, and how do they get an inspection?" | `housing_conditions_health_guide` | Pests (410.550, owner responsible in a building with 2 or more units; urgent under 410.630(A)(17)) and moisture/mold (410.500(A)(1), 48-hour drying, 410.620(E)); inspection within 1 business day for pests (410.600(C)); health effects cited (EPA, CDC); tenant protections with the not-legal-advice notice; Holyoke housing facts; teaching sheets in `output/` with reading grade at or below 8. |
| 5 | "Make tenant teaching sheets on no heat and lead paint in English and Spanish." | `housing_conditions_health_guide` | English sheets written; Spanish listed as "no agency translation found: English only" unless `/data/templates/spanish/housing_heat.md` (or `housing_lead.md`) exists, in which case that file is used. No machine translation. Heat sheet: 68°F day, 64°F night, September 15 to May 31. |
| 6 | "Fill inspection request and repair request letters from housing_requests.csv." | `housing_conditions_health_guide` | Letters in `output/housing_letters…`; the chat shows only counts (3 households, 6 letters, English fallback for Spanish and Portuguese) and file names. No tenant names, addresses, or phone numbers in the chat. Open a letter and check it reads well. |
<!-- End of Phase 14 vectorhousing prompts. -->
<!-- Phase 14 Chemicals and food (ENV-08, CHR-06) prompts. -->
<!-- End of Phase 14 chemfood prompts. -->

## Content review (the subject matter expert)

All Phase 14 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

<!-- Phase 14 Water (ENV-03, CHR-07) review items. -->
<!-- End of Phase 14 water review items. -->
<!-- Phase 14 EJ and climate (ENV-04, ENV-05) review items. -->
<!-- End of Phase 14 ejclimate review items. -->
<!-- Phase 14 Vectors and housing (ENV-06, ENV-07) review items. -->
1. `content/env/vector_borne.yaml`: risk level meanings and public actions against DPH's current
   dashboard; DPH's recommended response at each level (shortened from the 2026 plan's Tables 1
   and 2; the WNV Critical row is read as "in addition to High"); the 2026 cancellation times
   (transcribed from calendar images in Appendix 2, medium confidence); the camp tick-check
   suggestion (this project's, medium). DPH's tick page and mosquito repellent fact sheet give
   different DEET limits for children (10–15% versus 30% or less): decide which message to use.
   DPH's 2021 school repellent guidance was written for an EEE outbreak year; confirm it is still
   current.
2. `content/env/housing_conditions.yaml`: every code section and number against 105 CMR 410.000
   (Mass. Register #1498, 6/23/2023), including the effective date; the tenant protection
   summaries against the statutes and DPH's Notice of Occupants' Legal Rights (have counsel or
   legal aid read them); the seven tenant teaching sheets and two letter templates (this
   project's drafts) for accuracy, tone, and reading level; then add agency-approved
   translations in `/data/templates/<language>/housing_<sheet>.md`.
3. Snapshots `dph-arbovirus-risk` and `dph-tickborne-surveillance`: spot-check a few towns and
   counties against DPH's workbooks and reports. DPH's 2025 summary shows the same count (167)
   and rate for babesiosis and anaplasmosis in Barnstable County, and Dukes and Nantucket Lyme
   cases fell from 263 (2024) to 40 (2025); both are copied as published.
<!-- End of Phase 14 vectorhousing review items. -->
<!-- Phase 14 Chemicals and food (ENV-08, CHR-06) review items. -->
<!-- End of Phase 14 chemfood review items. -->
