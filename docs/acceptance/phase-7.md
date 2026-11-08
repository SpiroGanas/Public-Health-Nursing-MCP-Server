# Acceptance script: Phase 7 (school health)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `school` toolset enabled (`PHN_PROFILE=school_nurse`). Record pass/fail
and notes. The rules, calculations, and privacy guardrails are also checked automatically
(`crates/phn-server/tests/phase7_tools.rs`, the leak test in `mcp_integration.rs`, and the unit
tests in `crates/phn-tools/src/school/`); this script checks that the answers are correct for
Massachusetts school nursing practice and useful before Pilot 2.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Which health screenings are required in grade 7?" | `get_ma_school_screening_requirements` | Vision once in grades 6–8, hearing once in 6–8, BMI (grade 7), postural (grades 5–9), SBIRT if the district chose grade 7; each with its 105 CMR 200.000 or M.G.L. c. 71 citation; review caveat. |
| 2 | "What is the vision screening critical line for a 4-year-old, and which charts are allowed?" | `get_ma_school_screening_requirements` | 20/40 for preschool age 4 and older; LEA SYMBOLS and Sloan Letters (logMAR); instrument screening only ages 3–5 with near acuity still required (DPH 2022 protocol). |
| 3 | "Can a parent opt their child out of BMI screening? Who may see the results?" | `get_ma_school_screening_requirements` | Written opt-out (200.500(D)); results disclosed only to the parent or guardian without written permission (200.500(A)). |
| 4 | "Check school_screenings.csv as of November 15, 2026. Who is due, and what are our referral follow-up rates?" | `track_school_screenings` | Counts and rates by grade only, small counts as `*`; no names, IDs, birth dates, or phone numbers; `school_screenings_<date>.xlsx` in `output/` with an action list. Open it and check three students by hand against the grade rules. |
| 5 | "Write referral letters for the open vision and hearing referrals. I'm Nurse Example at 555-0100, Aldergrove Elementary." | `track_school_screenings` | Letters in a folder in `output/`, grouped by language; the answer says how many fell back to English because no agency translation exists, and how to add one in `/data/templates/<language>/`. Open a letter: names filled, plain language, cites c. 71 section 57 for the eye report. |
| 6 | Copy `template_english_vision_referral.md` to `/data/templates/spanish/vision_referral.md` (edit a word so you can tell it apart), then repeat prompt 5. | `track_school_screenings` | Spanish-speaking families now get the agency template; the fallback count drops. |
| 7 | "Can a trained secretary give a student's EpiPen on a field trip? What do we report afterwards?" | `school_medication_administration_guide` | Registered program, trained staff when a nurse is not immediately available (210.010(A)); 911 then caregiver; DPH Epinephrine Administration Report each time; field trips are regular school activities and the nurse decides coverage (210.005(E)(1)(o)). |
| 8 | "How long can we keep a student's medication at school, and at what refrigerator temperature?" | `school_medication_administration_guide` | No more than a 30-school-day supply except multidose devices; 36–46 °F (2–8 °C); locked storage; cites 210.008. |
| 9 | "Give me an IHP and emergency action plan template for a student with type 1 diabetes, with 504 suggestions." | `build_individualized_health_plan` | Recognition (below 70 mg/dL), the 15-15 rule, glucagon and 911, nursing assessment, goals, interventions, 210.005(E) plan elements, suggested 504 accommodations labeled as suggestions for the team; every item cited. |
| 10 | "Fill food allergy plans for the students in health_plan_students.csv." | `build_individualized_health_plan` | Counts only (2 students, other conditions skipped), blank-field counts, languages; plans in `output/health_plans_food_allergy/`; no student details in the chat. |
| 11 | "A soccer player was hit Friday, October 2, 2026, and diagnosed with a concussion the next day. She has headaches and light sensitivity. What is due, and what accommodations should teachers use?" | `concussion_return_to_learn_tracker` | Removal from play; parent told right away and in writing by Monday, October 5; AD and nurse by Monday; Report of Head Injury form; reentry plan; accommodations for headaches and light sensitivity; DPH vs CDC difference on days out of school shown. |
| 12 | "Check concussion_log.csv and write reentry plans for the diagnosed students." | `concussion_return_to_learn_tracker` | Counts of records missing each requirement (for example clearance by a coach is not allowed), 201.017 statistics (suppressed if small), files in `output/`; no names or emails. |
| 13 | "Is there unusual illness absenteeism in school_absence_log.csv? Should I call the board of health?" | `monitor_school_absenteeism` | A gastrointestinal signal at Aldergrove Elementary in the last days; small counts as `<5`; 105 CMR 300.134 and DPH's online GI cluster form; says a signal is not an outbreak decision. |
| 14 | "Look for clusters by grade in the same file, stomach illness only." | `monitor_school_absenteeism` | Signals by school and grade; grouping by a name column is refused. |
| 15 | "Build the 2025–2026 annual report from health_office_visits.csv and school_screenings.csv; enrollment is 420." | `build_school_health_annual_report` | Monthly table and returned-to-class rate; BMI counts by grade, sex, and category with data-quality checks; DPH report list with due dates; workbook and committee summary in `output/`; says DPH's current Status Report form is not public. Open the committee summary and check it reads well for a school committee. |
| 16 | "Give me the medication checklist for a large group child care program and record that medication training is not met." | `child_care_health_consultation_checklist` | 606 CMR 7.11(1)(b) and 7.11(2) items with evidence to look for; one gap listed; worksheet if asked. |

## Content review (the subject matter expert)

All school health content is `status: pending` (review due 2027-02-01). Before Pilot 2, check each
file against its cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

1. `content/school/screening_requirements.yaml` against 105 CMR 200.000, M.G.L. c. 71, sections
   57 and 97, and DPH's 2022 vision protocol. **Hearing:** replace the 2007 manual's details
   (marked medium/low) with DPH's current Pure Tone Audiometer Hearing Screening Protocol, which
   could not be retrieved; resolve the 1-week vs 2-week rescreen wording. Confirm whether DPH still
   collects the Postural Screening Final Report (July 15) and the current BMI reporting route.
2. `content/school/medication_administration.yaml` against 105 CMR 210.000 (12/5/2025) and the
   Board of Registration in Nursing advisory rulings AR 92-05 and AR 9324 (cited by name only).
3. `content/school/concussion.yaml` against 105 CMR 201.000, DPH's 2018 return-to-learn
   guidelines and current clearance form, and CDC HEADS UP. **Decide** whether the district follows
   DPH 2018 (rest 2–4 days) or CDC 2025 (most return in 1–2 days) for time out of school, and
   update the year-end report deadline each year.
4. `content/school/health_plans.yaml`: the goals, interventions, and suggested 504
   accommodations are a nursing template organized from cited CDC and DPH statements (medium
   confidence). Check them against your practice and your district's plan forms; note CDC's two
   severe-hypoglycemia thresholds (54 vs 55 mg/dL).
5. `content/school/absenteeism.yaml`: the keyword lists that sort absence reasons into syndromes
   are tool defaults; adjust them to how your district records reasons.
6. `content/school/annual_report.yaml`: obtain DPH's current Status Report and Monthly Activities
   Report forms from the School Health Services epidemiologist and check the workbook's
   categories against them.
7. `content/school/child_care_checklist.yaml` against 606 CMR 7.00 as printed (the consent table
   in 7.11(2)(l) was reconstructed from flattened PDF text).
8. `content/school/cdc_bmi_for_age.yaml` is copied unchanged from CDC's `bmiagerev.csv` (checksum
   in the file header); confirm the BMI categories DPH's report uses.
