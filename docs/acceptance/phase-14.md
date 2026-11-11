# Acceptance script: Phase 14 (environmental health and climate)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `env`, `chronic`, `equity`, and `core` toolsets enabled (for example
`PHN_TOOLSETS=env,chronic,equity,epi,core`). Record pass/fail and notes.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
<!-- Phase 14 Water (ENV-03, CHR-07) prompts. -->
| 1 | "Is there PFAS in Boxborough's drinking water, and what should pregnant residents do?" | `drinking_water_quality_lookup` | Lists Boxborough's community systems with PFAS6 results and flags those whose latest-quarter average is above 20 ng/L as informational (not compliance); gives MassDEP's advice for pregnant and nursing people and infants; says whether EPA SDWIS and the EEA portal were live or the snapshot (with its date). |
| 2 | "Show health-based drinking water violations for Acton over the last 10 years." | `drinking_water_quality_lookup` (`years` 10) | Violations are named (contaminant, type, rule), with return-to-compliance dates; other violations are counted by category; no operator names, emails, or phone numbers appear. |
| 3 | "What should private well owners in Harvard test for, and how often?" | `drinking_water_quality_lookup` (`private_wells` true) | Annual coliform and nitrate/nitrite; initial and 10-year list with MassDEP limits; PFAS testing; certified labs; board of health role; Harvard flagged on MassDEP's arsenic/uranium list. |
| 4 | "Look up water system MA2002000, including schools with their own wells." | `drinking_water_quality_lookup` | One system (Acton Water District) with lead 90th percentile against the 15 µg/L action level and the action-level note that an exceedance is not itself a violation. |
| 5 | "Is Hingham's water fluoridated?" | `community_water_fluoridation_status` | DPH 2023 (fully fluoridated) and CDC's 2025 records, with the note that CDC lists the Weir River system twice with different answers; advises asking the water system. |
| 6 | "Prepare a fluoridation brief for our board of health in Fall River." | `community_water_fluoridation_status` (`brief` true) | DOCX in `/data/output`; flags the 0.00 mg/L 2025 average and DPH's interim suspension note; positions side by side with dates (DPH, CDC, CPSTF, EPA, FDA, AAP) without choosing; NTP, JAMA Pediatrics, Cochrane, CDC evidence; c. 111 § 8C steps; draft discussion points; lists the bodies not included. |
| 7 | "What share of Massachusetts residents on public water get fluoridated water?" | `community_water_fluoridation_status` (`town` "Massachusetts") | 58.2% of people on community systems (CDC, November 2025), systems by status, towns by DPH status. |
<!-- End of Phase 14 water prompts. -->
<!-- Phase 14 EJ and climate (ENV-04, ENV-05) prompts. -->
<!-- End of Phase 14 ejclimate prompts. -->
<!-- Phase 14 Vectors and housing (ENV-06, ENV-07) prompts. -->
<!-- End of Phase 14 vectorhousing prompts. -->
<!-- Phase 14 Chemicals and food (ENV-08, CHR-06) prompts. -->
<!-- End of Phase 14 chemfood prompts. -->

## Content review (the subject matter expert)

All Phase 14 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

<!-- Phase 14 Water (ENV-03, CHR-07) review items. -->
- `content/env/drinking_water.yaml`: the PFAS6 standard and compliance method (MassDEP Quick
  Reference Guide, 310 CMR 22.07G); the federal PFAS status (EPA's May 18, 2026 proposals, which
  EPA expected to finalize by the end of 2026: re-check); lead and copper action levels and the
  LCRI dates; MassDEP's PFAS advice for consumers (fact sheet dated 2026-05-28); EPA's lead steps;
  private well Tables 9, 11, and 12 and the PFAS FAQ. Decide whether the tool default of 90% (share
  of residents on community systems below which private wells are suggested) suits you.
- `content/chronic/fluoridation.yaml`: each position with its date (DPH statement, CDC pages
  dated 2024-05-15, CPSTF as described by CDC (medium confidence), EPA review pages of 2026, FDA
  October 31, 2025, AAP November 3, 2025); the evidence summaries (NTP 2024, JAMA Pediatrics 2025,
  Cochrane 2024, CDC 2024); the c. 111 § 8C summary; and the draft discussion points (medium
  confidence). Decide whether to add positions the tool could not fetch (ADA, ASTDD, AAPD, WHO,
  HHS) and in what order bodies should appear (the tool lists DPH first, as for vaccines).
- Snapshots `cdc-mwf-ma`: spot-check the town assignment of CDC records labeled "named part of a
  town", "PWSID town number", and "duplicate of" in `snapshots/ma_fluoridation_systems.csv`.
<!-- End of Phase 14 water review items. -->
<!-- Phase 14 EJ and climate (ENV-04, ENV-05) review items. -->
<!-- End of Phase 14 ejclimate review items. -->
<!-- Phase 14 Vectors and housing (ENV-06, ENV-07) review items. -->
<!-- End of Phase 14 vectorhousing review items. -->
<!-- Phase 14 Chemicals and food (ENV-08, CHR-06) review items. -->
<!-- End of Phase 14 chemfood review items. -->
