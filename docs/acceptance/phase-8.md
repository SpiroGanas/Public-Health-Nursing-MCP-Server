# Acceptance script: Phase 8 (maternal, child, and family health)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `mch`, `equity`, and `core` toolsets enabled (for example
`PHN_TOOLSETS=mch,equity,core`). Record pass/fail and notes. The calculations and privacy rules
are also checked automatically (`crates/phn-server/tests/phase8_tools.rs`, the leak test in
`mcp_integration.rs`, `crates/phn-tools/tests/growth_golden.rs`, and the unit tests in
`crates/phn-tools/src/mch/` and `equity/benefits.rs`); this script checks that the answers are
correct for Massachusetts practice and useful on a home visit.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "A 9-month-old boy weighs 9.7 kg and is 72 cm long. What are his percentiles?" | `calculate_growth_percentiles` | WHO 2006 chart; weight-for-age z about +0.8 (about the 79th percentile); length-for-age, weight-for-length, and BMI shown; WHO ±2 SD cut points explained. |
| 2 | "A 10-year-old girl is 55 inches tall and weighs 110 pounds. BMI percentile?" | `calculate_growth_percentiles` | CDC 2000 chart with the 2022 extended BMI method; BMI about 25.6; obesity category; % of the 95th percentile shown. |
| 3 | "Plot this girl's weights: 2 weeks 3.4 kg, 1 month 4.0 kg, 2 months 4.9 kg. She was born at 34 weeks." | `calculate_growth_percentiles` | Corrected age used (6 weeks early) with the low-confidence corrected-age caveat; z-score change between visits. |
| 4 | "Score growth_measurements.csv." | `calculate_growth_percentiles` | Counts by age group and weight status only (small counts `*`); 2 rows not scored; no names, IDs, phones, or birth dates in the chat; results file in `output/`. Spot-check two rows against CDC's or WHO's tables. |
| 5 | "An 18-month-old in Lowell had a fingerstick lead level of 7. What next?" | `childhood_lead_screening_guide` | Venous confirmation within 3 months (DPH recommends within 2 months for 5+); level of concern actions; Lowell on the 2024 high-risk list (age-4 screening) with its screening rate. |
| 6 | "Venous lead level 16 in a 2-year-old: case management steps? Make a teaching sheet in Spanish." | `childhood_lead_screening_guide` | Lead poisoned; mandatory CLPPP inspection; care coordinator; retest within 1-2 months; siblings tested; report within 3 working days. Teaching sheet in `output/` in English with a note that no agency Spanish translation was found (then add `templates/spanish/lead_family_teaching.md` and repeat: the agency file is used). |
| 7 | "I saw unexplained bruises on a toddler at a home visit. Do I have to report, and how? I called DCF at 3:20 pm today." | `mandated_reporter_51a_guide` | Nurse is a mandated reporter; call DCF now, written report due 48 hours after the call (date and time shown); contents; protections. |
| 8 | "Draft the written 51A report from 51a_notes.txt." | `mandated_reporter_51a_guide` | Only the file name and field counts in the chat; no names, address, or quotes; open the DOCX in `output/` and check every field and the `?` marks. |
| 9 | "Show the milestones for an 18-month-old. She doesn't say 3 words yet (18m-language-1). We're in Brockton." | `check_developmental_milestones` | CDC 18-month checklist with IDs; act early; autism and general screening due; EI eligibility and no cost; EI program(s) for Brockton with phone. |
| 10 | "Plan a home visit for a 5-day-old breastfed baby; birth weight 7 lb 8 oz, today 6 lb 15 oz; failed hearing screen. Make the documents." | `plan_newborn_postpartum_visit` | Weight change about −7.5% (reported, not judged); hearing follow-up; safe sleep; car seat law; blood spot; Hear Her warning signs; plan and teaching sheet in `output/` with no family information. |
| 11 | "A family of 3 with a newborn earns $3,500 a month. What might they qualify for?" | `estimate_benefit_eligibility` | 154% FPL; WIC, SNAP, MassHealth (infant, children) likely income-eligible; parents over 133%; HSN with deductible; HEAP; effective dates; "screen, not a determination". |
| 12 | "Which towns in Bristol County should we prioritize for lead outreach? Make a map." | `map_lead_risk` | Ranked table with pre-1950 housing, low income, screening gap, elevated BLL; New Bedford, Fall River, and Taunton marked DPH high-risk; method stated as the server's index; map in `output/maps/`. |
| 13 | "Give me a maternal and infant health profile for Springfield." | `maternal_infant_health_profile` | 2023 births, low birthweight and preterm % with 95% CIs vs state and U.S.; infant mortality by race (2022) with ratios; PRAMS; U.S. periods labeled. |

## Content review (the subject matter expert)

All Phase 8 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

1. `content/mch/lead_screening.yaml` against 105 CMR 460.000 (Mass. Register #1353, 12/1/17) and
   DPH CLPPP's current pages: definitions, schedule, high-risk criteria, confirmation and
   follow-up bands, actions by level, reporting deadlines, and the 2024 high-risk list. Decide
   whether to keep DPH's combined table (Massachusetts rules plus CDC's 3.5 µg/dL reference
   value). Set the annual refresh for the high-risk list and the `ma-lead-screening` snapshot
   (24 communities show 2023 data because DPH's 2024 PDF omitted their page).
2. `content/mch/mandated_reporting.yaml` against M.G.L. c. 119, ss. 21 and 51A and DCF's
   Mandated Reporter Guide. **Decide** how to present prenatal substance exposure: s. 51A(a) says
   it alone does not meet the requirement, but DCF's guide (2025) still lists newborn drug
   dependence. Review the written-report template sections against DCF's online form.
3. `content/mch/developmental_milestones.yaml`: milestones are transcribed word for word from
   CDC's pages (updated 2026-05-16); check the act-early steps, the AAP screening ages, and the
   Massachusetts EI text. Decide how to handle prematurity (the tool shows both checklists).
4. `content/mch/growth_interpretation.yaml`: chart selection, WHO ±2 SD and CDC 5th/95th cut
   points, BMI categories, extreme-value cut points. **Decide** the corrected-age rule (the
   draft corrects until 24 months for births before 37 weeks; low confidence, no CDC source).
5. `content/mch/newborn_postpartum.yaml`: Hear Her signs, safe sleep (CDC page and AAP 2022
   abstract), feeding, 105 CMR 270.007, M.G.L. c. 111, s. 67F, c. 90, s. 7AA. **The jaundice
   items are low confidence** (the AAP 2022 guideline text could not be retrieved): replace them
   with the guideline's follow-up recommendations or your agency protocol. Decide whether the
   plan should include a weight-loss threshold (none is given).
6. `content/equity/benefit_eligibility.yaml`: FPL, WIC, SNAP (confirm the October 1, 2026 date),
   MassHealth groups, HSN, HEAP. Set the refresh schedule owner.
7. The data snapshots' derivations in `snapshots/manifest.json` (`cdc-growth-charts`,
   `who-growth-standards`, `ma-lead-screening`, `ma-ei-programs`, `ma-acs5-housing-age`,
   `ma-maternal-infant`).

## Translations

The server ships English family sheets only. To use the agency's approved translations, save
them in the data folder as `templates/<language>/lead_family_teaching.docx` (or `.md`) and
`templates/<language>/newborn_postpartum_teaching.docx` (or `.md`), where `<language>` is a folder
name such as `spanish`, `es`, `portuguese`, or `haitian_creole`. When a family's language has no
agency file, the English sheet is written and the answer says so.
