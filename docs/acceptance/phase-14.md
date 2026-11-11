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
