# Acceptance script: Phase 5 (immunizations I)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `vax` and `core` toolsets enabled (for example `PHN_PROFILE=school_nurse`
or `PHN_TOOLSETS=vax`). Record pass/fail and notes. The rules and calculations are also checked
automatically (`crates/phn-server/tests/phase5_tools.rs` and the unit tests in
`crates/phn-tools/src/vax/`); this script checks that the answers are correct for Massachusetts
practice and useful.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "What is the recommended schedule for hepatitis B in infants?" | `get_immunization_schedule` | DPH first (universal birth dose), then AAP, then CDC/ACIP, each with a date; minimum ages and intervals; review caveat. |
| 2 | "How do DPH, AAP, and CDC differ on the hepatitis B birth dose?" | `compare_vaccine_recommendations` | Side-by-side positions with dates and links; the difference is highlighted. |
| 3 | "Set my vaccine recommendation basis to CDC/ACIP," then repeat prompt 1. | `configure_jurisdiction`, `get_immunization_schedule` | CDC/ACIP listed first; a caveat that MA school requirements follow DPH. Set it back to the default afterwards. |
| 4 | "Does a kindergartner born 3/15/2020 with DTaP on 5/15/20, 7/15/20, 9/15/20, 6/15/21, and 3/5/24 meet the DTaP requirement?" | `check_ma_school_immunization_requirements` | Not met: the 5th dose was 10 days before the 4th birthday; a dose can be given now. |
| 5 | "Check school_roster.csv against the school requirements for September 2, 2026." | `check_ma_school_immunization_requirements` | Counts by grade and requirement only, small counts shown as `*`; no names, IDs, or birth dates; results file in `output/`. Open the file and spot-check three students by hand. |
| 6 | "Which kindergartens in Hampshire County are below 95% for MMR?" | `analyze_vaccination_coverage` | Schools ranked lowest first; schools DPH did not release are counted, not ranked; MA vs U.S. comparison with confidence intervals. |
| 7 | "Our fridge read 1.0 to 9.5 °C for 45 minutes. What do I do? Make me a worksheet." | `vaccine_temperature_excursion_helper` | Reportable to the MA Vaccine Program (cumulative); CDC and DPH steps; says only the manufacturer decides viability; XLSX in `output/`. |
| 8 | "Look up NDC 00006-4681-00." | `lookup_vaccine_product` | M-M-R II, CVX 03, Merck (MSD); current VIS date; translations link. |
| 9 | "Plan a flu clinic for 300 people over 4 hours, and what epinephrine dose for a 20 kg child?" | `plan_vaccination_clinic` | Staffing by station with the arithmetic shown; assumptions labeled as source or tool default; 0.2 mg (0.2 mL of 1 mg/mL), band 5–7 years, 0.15 mg autoinjector; reconciliation sheet in `output/`. |
| 10 | "A parent says vaccines cause autism. How should I respond?" | `vaccine_confidence_conversation_guide` | Open question, reflection, permission, cited facts, next step; notes the changed CDC page; reading grade shown. |
| 11 | "Same, but the parent speaks Portuguese." | `vaccine_confidence_conversation_guide` | Recommends a qualified interpreter; links translated resources; no machine translation. |
| 12 | "Any recalls or shortages for amoxicillin?" | `lookup_medication_information` | Live openFDA data with dates and set IDs, or a clear "not checked" if offline; reference-only notice. |

## Content review (the subject matter expert)

All immunization content is `status: pending`. Before Pilot 2, check each file against its cited
source, then set `meta.review` (see `CONTRIBUTING.md`):

1. `content/vax/ma_school_requirements.yaml` against DPH's 2026–2027 requirements document and
   105 CMR 220.000, and set the annual refresh date (DPH posts new requirements each year). Two
   decisions are needed (see `meta.verification_note`): whether the grade 7–10 MenACWY dose must
   be on or after the 10th or the 11th birthday, and whether the 4-day grace period applies to
   school requirements.
2. `content/vax/recommendations.yaml` and `content/vax/immunization_schedule.yaml`: each body's
   position and date, especially those marked `confidence: medium`.
3. `content/vax/clinic_planning.yaml`: the epinephrine and diphenhydramine tables against the
   signed DPH standing orders and Immunize.org P3082a/P3082; replace the `tool_default`
   throughput figures with your agency's experience.
4. `content/vax/storage_handling.yaml`, `vaccine_products.yaml`, `confidence_guide.yaml`, and
   `coverage_thresholds.yaml`.
