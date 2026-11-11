# Acceptance script: Phase 14 (environmental health and climate)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `env`, `chronic`, `equity`, `epi`, and `core` toolsets enabled (for example
`PHN_TOOLSETS=env,chronic,equity,epi,core`). Record pass/fail and notes.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Is there PFAS in Boxborough's drinking water, and what should pregnant residents do?" | `drinking_water_quality_lookup` | Lists Boxborough's community systems with PFAS6 results and flags those whose latest-quarter average is above 20 ng/L as informational (not compliance); gives MassDEP's advice for pregnant and nursing people and infants; says whether EPA SDWIS and the EEA portal were live or the snapshot (with its date). |
| 2 | "Show health-based drinking water violations for Acton over the last 10 years." | `drinking_water_quality_lookup` (`years` 10) | Violations are named (contaminant, type, rule), with return-to-compliance dates; other violations are counted by category; no operator names, emails, or phone numbers appear. |
| 3 | "What should private well owners in Harvard test for, and how often?" | `drinking_water_quality_lookup` (`private_wells` true) | Annual coliform and nitrate/nitrite; initial and 10-year list with MassDEP limits; PFAS testing; certified labs; board of health role; Harvard flagged on MassDEP's arsenic/uranium list. |
| 4 | "Look up water system MA2002000, including schools with their own wells." | `drinking_water_quality_lookup` | One system (Acton Water District) with lead 90th percentile against the 15 µg/L action level and the action-level note that an exceedance is not itself a violation. |
| 5 | "Is Hingham's water fluoridated?" | `community_water_fluoridation_status` | DPH 2023 (fully fluoridated) and CDC's 2025 records, with the note that CDC lists the Weir River system twice with different answers; advises asking the water system. |
| 6 | "Prepare a fluoridation brief for our board of health in Fall River." | `community_water_fluoridation_status` (`brief` true) | DOCX in `/data/output`; flags the 0.00 mg/L 2025 average and DPH's interim suspension note; positions side by side with dates (DPH, CDC, CPSTF, EPA, FDA, AAP) without choosing; NTP, JAMA Pediatrics, Cochrane, CDC evidence; c. 111 § 8C steps; draft discussion points; lists the bodies not included. |
| 7 | "What share of Massachusetts residents on public water get fluoridated water?" | `community_water_fluoridation_status` (`town` "Massachusetts") | 58.2% of people on community systems (CDC, November 2025), systems by status, towns by DPH status. |
| 8 | "Which neighborhoods in Lowell are environmental justice populations, and which languages should we plan for in outreach?" | `environmental_justice_screen` | 77 of 81 block groups, 111,001 people (96.1% of residents, EEA's figure); counts by criterion and combination; English-isolation block groups with limited-English households; the largest limited-English language groups (ACS); MEPA rules (1- and 5-mile area, translation at 5%, interpreter above 10%, consult the Board of Health); DPH's four vulnerable health criteria; caveats that the layer is 2020/ACS 2016–2020 and that EJScreen is archived and not bundled. |
| 9 | "How many people in Hampden County live in EJ block groups? Make a map and a table." | `environmental_justice_screen` | County total and percent of 2020 Census residents; towns ranked (Springfield first); SVG and PNG map in `output/maps/` shaded by number of criteria with town outlines; XLSX of block groups in `output/`. Open the map and check that it reads well and has alt text. |
| 10 | "Use ejscreen_ma.csv to compare air pollution and traffic percentiles in Chelsea's EJ block groups with the rest of the city." (Save an archived EJScreen 2.3 Massachusetts extract first; then repeat with `line_list.csv` as the file.) | `environmental_justice_screen` | Population-weighted average percentiles for EJ and other block groups and counts at or above the 80th percentile; the 80th percentile is described as EPA's former starting point, not a designation; no rows or values from the file. With `line_list.csv`: refused, and no names, phones, or other values from the file appear. |
| 11 | "Build a climate and health vulnerability profile for Scituate for our MVP workshop, as a Word document." Then the same for Chelsea. | `climate_health_vulnerability_profile` | Scituate: NFIP claims history (thousands of claims; flagged), NFIP and CRS status, FEMA hazard mitigation plans with expiration dates (live online; "snapshot" offline), MVP region and planning report. Chelsea: high impervious surface and low tree canopy, EJ share, and PLACES or ACS indicators flagged. Each indicator shows the Massachusetts value, source, and vintage; the rules are labeled tool defaults; DOCX in `output/`. |
| 12 | "What is the EEE and West Nile virus risk in Middleborough right now, and what should we tell the schools?" | `vector_borne_disease_risk` | Both levels for Middleborough with the date DPH last updated them. Outside June to October (or offline) the answer says the season is over (or not started) and that the levels are NOT current risk; in season and online it says the levels are current. What the level means, residents' actions, and DPH's plan response match DPH's dashboard and the plan's Tables 1–2. School messages cite DPH's repellent guidance for school staff. Plymouth County tick-borne disease table with Massachusetts rates. |
| 13 | "Which towns in Plymouth County were at moderate or higher EEE risk at the end of the 2024 season, and how many human EEE cases were there?" | `vector_borne_disease_risk` | 2024 final levels (as of 2024-10-30); towns listed; human cases shown only as county counts (no ages, sexes, or towns). Compare two towns with DPH's 2024 workbook. |
| 14 | "How common are babesiosis and anaplasmosis in Barnstable County, and is it getting worse? Make a one-page notice for camps." | `vector_borne_disease_risk` | 2022–2025 counts and DPH rates for Barnstable with the state rate and change from the previous year; counts under 20 flagged; ED visit series; camp messages cite 105 CMR 430.159(B)(4) and 430.401(A); DOCX notice in `output/`. |
| 15 | "A family in Holyoke has roaches and a leak in the bathroom ceiling. What does the sanitary code require, who has to fix it, and how do they get an inspection?" | `housing_conditions_health_guide` | Pests (410.550, owner responsible in a building with 2 or more units; urgent under 410.630(A)(17)) and moisture/mold (410.500(A)(1), 48-hour drying, 410.620(E)); inspection within 1 business day for pests (410.600(C)); health effects cited (EPA, CDC); tenant protections with the not-legal-advice notice; Holyoke housing facts; teaching sheets in `output/` with reading grade at or below 8. |
| 16 | "Make tenant teaching sheets on no heat and lead paint in English and Spanish." | `housing_conditions_health_guide` | English sheets written; Spanish listed as "no agency translation found: English only" unless `/data/templates/spanish/housing_heat.md` (or `housing_lead.md`) exists, in which case that file is used. No machine translation. Heat sheet: 68°F day, 64°F night, September 15 to May 31. |
| 17 | "Fill inspection request and repair request letters from housing_requests.csv." | `housing_conditions_health_guide` | Letters in `output/housing_letters…`; the chat shows only counts (3 households, 6 letters, English fallback for Spanish and Portuguese) and file names. No tenant names, addresses, or phone numbers in the chat. Open a letter and check it reads well. |
| 18 | "A tanker truck spilled toluene on Route 20 in our town. What are the health effects, and whom do we call?" | `chemical_exposure_reference` | Toluene (CAS 108-88-3): NIOSH REL, PEL, IDLH (Pocket Guide and current table), symptoms, target organs, first aid; ATSDR MRLs by route and duration; the ToxFAQs link and PDF. The first lines say occupational limits are not community limits and MRLs are screening levels. Whom to call lists 911, MassDEP 1-888-304-1133, the National Response Center, Poison Control 1-800-222-1222, and DPH toxicology (617) 624-5757. Check the numbers against the linked pages. |
| 19 | "Residents near a closed dry cleaner are worried about perc in their indoor air. What are the screening levels?" | `chemical_exposure_reference` | "perc" matches tetrachloroethylene (127-18-4) with the indoor air scenario: inhalation MRLs (acute, intermediate, chronic) as screening levels, the DPH Indoor Air Quality unit, and no suggestion that the OSHA PEL is a safe home level. |
| 20 | "Look up arsenic for a contaminated soil question." | `chemical_exposure_reference` | The tool does not guess: it lists the inorganic and organic arsenic entries (and others) and asks which one; then "arsenic, inorganic compounds" or "7440-38-2" gives the entry with ATSDR's oral MRLs, the soil scenario's note that oral MRLs are doses (not soil concentrations), and MassDEP's contaminated property lookup. |
| 21 | "What is PFAS and what are the ATSDR levels? It was found in a private well." | `chemical_exposure_reference` | "PFAS" maps to the Perfluoroalkyls ToxFAQ with the oral MRLs for PFOA, PFOS, PFNA, and PFHxS; the drinking water scenario says private wells are regulated by the local board of health and points to `drinking_water_quality_lookup`. |
| 22 | "Which census tracts in Springfield are low income and low access to food? Compare walking distance and driving." | `food_access_mapper` | 37 tracts; by straight-line distance to any SNAP store 5 tracts meet the 1/2-mile definition and 1 the vehicle definition; by driving 12 and 3. The 2019 supermarket map (approximate) shows 25 tracts at 1/2 mile. Map (SVG/PNG), DOCX, and XLSX in `output/`. Check that the explanation of the two maps (any SNAP store vs. supermarkets) is clear. |
| 23 | "Map food access in Springfield and count our pantries from food_pantries.csv." | `food_access_mapper` | 4 Springfield sites (2 pantries/meal sites with coordinates, counted inside or outside low-access tracts), by kind; the Holyoke and Chicopee rows are not counted for Springfield. No organization names, addresses, phone numbers, or the contact people's names, cells, or emails appear in the chat; open the DOCX and XLSX and confirm the sites are listed there. |
| 24 | "Use line_list.csv as our pantry list for Springfield." | `food_access_mapper` | Refused as a list of people, naming only the column headers; no values from the file. |
| 25 | "Food access for Hampden County: HIP vendors and SNAP stores. I put snap_retailers.csv in my folder." (Download the Massachusetts SNAP retailer file from the USDA SNAP Retailer Locator first.) | `food_access_mapper` | 104 tracts; SNAP stores counted by town and by kind (supermarket, grocery, convenience, ...), retailers out of state or with an end date left out; HIP facts ($40/$60/$80 a month, farmers markets, farm stands, mobile markets, CSAs; DTAFinder.com) and how to build a HIP vendor list. Record whether the USDA file's column names were recognized. |

## Content review (the subject matter expert)

All Phase 14 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

1. `content/env/drinking_water.yaml`: the PFAS6 standard and compliance method (MassDEP Quick
   Reference Guide, 310 CMR 22.07G); the federal PFAS status (EPA's May 18, 2026 proposals, which
   EPA expected to finalize by the end of 2026: re-check); lead and copper action levels and the
   LCRI dates; MassDEP's PFAS advice for consumers (fact sheet dated 2026-05-28); EPA's lead steps;
   private well Tables 9, 11, and 12 and the PFAS FAQ. Decide whether the tool default of 90% (share
   of residents on community systems below which private wells are suggested) suits you.
2. `content/chronic/fluoridation.yaml`: each position with its date (DPH statement, CDC pages
   dated 2024-05-15, CPSTF as described by CDC (medium confidence), EPA review pages of 2026, FDA
   October 31, 2025, AAP November 3, 2025); the evidence summaries (NTP 2024, JAMA Pediatrics 2025,
   Cochrane 2024, CDC 2024); the c. 111 § 8C summary; and the draft discussion points (medium
   confidence). Decide whether to add positions the tool could not fetch (ADA, ASTDD, AAPD, WHO,
   HHS) and in what order bodies should appear (the tool lists DPH first, as for vaccines).
3. Snapshots `cdc-mwf-ma`: spot-check the town assignment of CDC records labeled "named part of a
   town", "PWSID town number", and "duplicate of" in `snapshots/ma_fluoridation_systems.csv`.
4. `content/env/environmental_justice.yaml`: the four criteria and the neighborhood definition
   against St. 2021, c. 8, § 56; the MEPA designated geographic area, EIR, advance notice, and
   language rules against the two MEPA EJ protocols (the PDF protocols were not fetched; the
   guide page was); DPH's vulnerable health criteria against the DPH EJ Tool; the EJScreen column
   list (medium confidence: from EPA's column file as copied in the EJAM repository); the two
   "how to use" items (this project's drafts).
5. `content/env/climate_vulnerability.yaml`: the flagging rules are tool defaults (1.2 times the
   state value, heat days 1.1, canopy 0.75, impervious 1.5, NFIP 2 times or 25 recent claims,
   EJ share 50%, plan expiry 12 months, at most 8 listed); decide whether they fit your MVP or
   hazard mitigation planning team. The ZIP-to-town weighting of Heat and Health Index values is
   this project's method (medium). Confirm your town's MVP designation with your regional
   coordinator: mass.gov lists planning reports, not designations.
6. `content/env/vector_borne.yaml`: risk level meanings and public actions against DPH's current
   dashboard; DPH's recommended response at each level (shortened from the 2026 plan's Tables 1
   and 2; the WNV Critical row is read as "in addition to High"); the 2026 cancellation times
   (transcribed from calendar images in Appendix 2, medium confidence); the camp tick-check
   suggestion (this project's, medium). DPH's tick page and mosquito repellent fact sheet give
   different DEET limits for children (10–15% versus 30% or less): decide which message to use.
   DPH's 2021 school repellent guidance was written for an EEE outbreak year; confirm it is still
   current.
7. `content/env/housing_conditions.yaml`: every code section and number against 105 CMR 410.000
   (Mass. Register #1498, 6/23/2023), including the effective date; the tenant protection
   summaries against the statutes and DPH's Notice of Occupants' Legal Rights (have counsel or
   legal aid read them); the seven tenant teaching sheets and two letter templates (this
   project's drafts) for accuracy, tone, and reading level; then add agency-approved
   translations in `/data/templates/<language>/housing_<sheet>.md`.
8. Snapshots `dph-arbovirus-risk` and `dph-tickborne-surveillance`: spot-check a few towns and
   counties against DPH's workbooks and reports. DPH's 2025 summary shows the same count (167)
   and rate for babesiosis and anaplasmosis in Barnstable County, and Dukes and Nantucket Lyme
   cases fell from 263 (2024) to 40 (2025); both are copied as published.
9. `content/env/chemical_exposure.yaml`: the two statements marked medium confidence (this
   project's wording that occupational limits are not safe community levels, and that an MRL is
   not a diagnostic threshold for one person) and the spill note contrasting AEGLs with
   occupational limits; every contact and phone number against its page; which contacts each
   scenario lists; and whether DPH wants another contact (for example the regional poison center
   by name). The PFAS search alias is a navigation aid.
10. `content/chronic/food_access.yaml`: the definitions and HIP and SNAP facts against the ERS and
   DTA pages; the 50 percent land-area rule for carrying the 2019 LRAM to 2020 tracts (tool
   default); the instructions for getting the SNAP retailer file (the USDA page could not be
   opened from the build network, low confidence) and the HIP vendor list (DTAFinder.com was
   blocked, low confidence); the column names the tool accepts.
11. Snapshots: spot-check `snapshots/niosh_npg.csv`, `niosh_idlh.csv`, and `atsdr_mrls.csv`
   against the NIOSH and ATSDR pages for a few chemicals you use often (benzene, toluene,
   tetrachloroethylene, trichloroethylene, chlorine, ammonia, carbon monoxide).
