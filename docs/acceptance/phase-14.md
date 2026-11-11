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
<!-- End of Phase 14 chemfood review items. -->
