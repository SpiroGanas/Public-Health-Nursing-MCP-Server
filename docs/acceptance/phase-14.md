# Acceptance script: Phase 14 (environmental health and climate)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `env`, `chronic`, `equity`, and `core` toolsets enabled (for example
`PHN_TOOLSETS=env,chronic,equity,epi,core`). Record pass/fail and notes.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
<!-- Phase 14 Water (ENV-03, CHR-07) prompts. -->
<!-- End of Phase 14 water prompts. -->
<!-- Phase 14 EJ and climate (ENV-04, ENV-05) prompts. -->
| 1 | "Which neighborhoods in Lowell are environmental justice populations, and which languages should we plan for in outreach?" | `environmental_justice_screen` | 77 of 81 block groups, 111,001 people (96.1% of residents, EEA's figure); counts by criterion and combination; English-isolation block groups with limited-English households; the largest limited-English language groups (ACS); MEPA rules (1- and 5-mile area, translation at 5%, interpreter above 10%, consult the Board of Health); DPH's four vulnerable health criteria; caveats that the layer is 2020/ACS 2016–2020 and that EJScreen is archived and not bundled. |
| 2 | "How many people in Hampden County live in EJ block groups? Make a map and a table." | `environmental_justice_screen` | County total and percent of 2020 Census residents; towns ranked (Springfield first); SVG and PNG map in `output/maps/` shaded by number of criteria with town outlines; XLSX of block groups in `output/`. Open the map and check that it reads well and has alt text. |
| 3 | "Use ejscreen_ma.csv to compare air pollution and traffic percentiles in Chelsea's EJ block groups with the rest of the city." (Save an archived EJScreen 2.3 Massachusetts extract first; then repeat with `line_list.csv` as the file.) | `environmental_justice_screen` | Population-weighted average percentiles for EJ and other block groups and counts at or above the 80th percentile; the 80th percentile is described as EPA's former starting point, not a designation; no rows or values from the file. With `line_list.csv`: refused, and no names, phones, or other values from the file appear. |
| 4 | "Build a climate and health vulnerability profile for Scituate for our MVP workshop, as a Word document." Then the same for Chelsea. | `climate_health_vulnerability_profile` | Scituate: NFIP claims history (thousands of claims; flagged), NFIP and CRS status, FEMA hazard mitigation plans with expiration dates (live online; "snapshot" offline), MVP region and planning report. Chelsea: high impervious surface and low tree canopy, EJ share, and PLACES or ACS indicators flagged. Each indicator shows the Massachusetts value, source, and vintage; the rules are labeled tool defaults; DOCX in `output/`. |
<!-- End of Phase 14 ejclimate prompts. -->
<!-- Phase 14 Vectors and housing (ENV-06, ENV-07) prompts. -->
<!-- End of Phase 14 vectorhousing prompts. -->
<!-- Phase 14 Chemicals and food (ENV-08, CHR-06) prompts. -->
| 1 | "A tanker truck spilled toluene on Route 20 in our town. What are the health effects, and whom do we call?" | `chemical_exposure_reference` | Toluene (CAS 108-88-3): NIOSH REL, PEL, IDLH (Pocket Guide and current table), symptoms, target organs, first aid; ATSDR MRLs by route and duration; the ToxFAQs link and PDF. The first lines say occupational limits are not community limits and MRLs are screening levels. Whom to call lists 911, MassDEP 1-888-304-1133, the National Response Center, Poison Control 1-800-222-1222, and DPH toxicology (617) 624-5757. Check the numbers against the linked pages. |
| 2 | "Residents near a closed dry cleaner are worried about perc in their indoor air. What are the screening levels?" | `chemical_exposure_reference` | "perc" matches tetrachloroethylene (127-18-4) with the indoor air scenario: inhalation MRLs (acute, intermediate, chronic) as screening levels, the DPH Indoor Air Quality unit, and no suggestion that the OSHA PEL is a safe home level. |
| 3 | "Look up arsenic for a contaminated soil question." | `chemical_exposure_reference` | The tool does not guess: it lists the inorganic and organic arsenic entries (and others) and asks which one; then "arsenic, inorganic compounds" or "7440-38-2" gives the entry with ATSDR's oral MRLs, the soil scenario's note that oral MRLs are doses (not soil concentrations), and MassDEP's contaminated property lookup. |
| 4 | "What is PFAS and what are the ATSDR levels? It was found in a private well." | `chemical_exposure_reference` | "PFAS" maps to the Perfluoroalkyls ToxFAQ with the oral MRLs for PFOA, PFOS, PFNA, and PFHxS; the drinking water scenario says private wells are regulated by the local board of health and points to `drinking_water_quality_lookup`. |
| 5 | "Which census tracts in Springfield are low income and low access to food? Compare walking distance and driving." | `food_access_mapper` | 37 tracts; by straight-line distance to any SNAP store 5 tracts meet the 1/2-mile definition and 1 the vehicle definition; by driving 12 and 3. The 2019 supermarket map (approximate) shows 25 tracts at 1/2 mile. Map (SVG/PNG), DOCX, and XLSX in `output/`. Check that the explanation of the two maps (any SNAP store vs. supermarkets) is clear. |
| 6 | "Map food access in Springfield and count our pantries from food_pantries.csv." | `food_access_mapper` | 4 Springfield sites (2 pantries/meal sites with coordinates, counted inside or outside low-access tracts), by kind; the Holyoke and Chicopee rows are not counted for Springfield. No organization names, addresses, phone numbers, or the contact people's names, cells, or emails appear in the chat; open the DOCX and XLSX and confirm the sites are listed there. |
| 7 | "Use line_list.csv as our pantry list for Springfield." | `food_access_mapper` | Refused as a list of people, naming only the column headers; no values from the file. |
| 8 | "Food access for Hampden County: HIP vendors and SNAP stores. I put snap_retailers.csv in my folder." (Download the Massachusetts SNAP retailer file from the USDA SNAP Retailer Locator first.) | `food_access_mapper` | 104 tracts; SNAP stores counted by town and by kind (supermarket, grocery, convenience, ...), retailers out of state or with an end date left out; HIP facts ($40/$60/$80 a month, farmers markets, farm stands, mobile markets, CSAs; DTAFinder.com) and how to build a HIP vendor list. Record whether the USDA file's column names were recognized. |
<!-- End of Phase 14 chemfood prompts. -->

## Content review (the subject matter expert)

All Phase 14 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

<!-- Phase 14 Water (ENV-03, CHR-07) review items. -->
<!-- End of Phase 14 water review items. -->
<!-- Phase 14 EJ and climate (ENV-04, ENV-05) review items. -->
1. `content/env/environmental_justice.yaml`: the four criteria and the neighborhood definition
   against St. 2021, c. 8, § 56; the MEPA designated geographic area, EIR, advance notice, and
   language rules against the two MEPA EJ protocols (the PDF protocols were not fetched; the
   guide page was); DPH's vulnerable health criteria against the DPH EJ Tool; the EJScreen column
   list (medium confidence: from EPA's column file as copied in the EJAM repository); the two
   "how to use" items (this project's drafts).
2. `content/env/climate_vulnerability.yaml`: the flagging rules are tool defaults (1.2 times the
   state value, heat days 1.1, canopy 0.75, impervious 1.5, NFIP 2 times or 25 recent claims,
   EJ share 50%, plan expiry 12 months, at most 8 listed); decide whether they fit your MVP or
   hazard mitigation planning team. The ZIP-to-town weighting of Heat and Health Index values is
   this project's method (medium). Confirm your town's MVP designation with your regional
   coordinator: mass.gov lists planning reports, not designations.
<!-- End of Phase 14 ejclimate review items. -->
<!-- Phase 14 Vectors and housing (ENV-06, ENV-07) review items. -->
<!-- End of Phase 14 vectorhousing review items. -->
<!-- Phase 14 Chemicals and food (ENV-08, CHR-06) review items. -->
1. `content/env/chemical_exposure.yaml`: the two statements marked medium confidence (this
   project's wording that occupational limits are not safe community levels, and that an MRL is
   not a diagnostic threshold for one person) and the spill note contrasting AEGLs with
   occupational limits; every contact and phone number against its page; which contacts each
   scenario lists; and whether DPH wants another contact (for example the regional poison center
   by name). The PFAS search alias is a navigation aid.
2. `content/chronic/food_access.yaml`: the definitions and HIP and SNAP facts against the ERS and
   DTA pages; the 50 percent land-area rule for carrying the 2019 LRAM to 2020 tracts (tool
   default); the instructions for getting the SNAP retailer file (the USDA page could not be
   opened from the build network, low confidence) and the HIP vendor list (DTAFinder.com was
   blocked, low confidence); the column names the tool accepts.
3. Snapshots: spot-check `snapshots/niosh_npg.csv`, `niosh_idlh.csv`, and `atsdr_mrls.csv`
   against the NIOSH and ATSDR pages for a few chemicals you use often (benzene, toluene,
   tetrachloroethylene, trichloroethylene, chlorine, ammonia, carbon monoxide).
<!-- End of Phase 14 chemfood review items. -->
