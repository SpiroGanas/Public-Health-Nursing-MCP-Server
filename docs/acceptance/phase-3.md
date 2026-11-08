# Acceptance and review: Phase 3 (communicable disease I) → Pilot 1

Phase 3 adds eight `cd` tools, the `investigate_outbreak` prompt, and four pending-review
content items. Two things must happen before Pilot 1:

1. **Content review (Part A).** The nurse (subject matter expert) verifies every isolation,
   quarantine, and exclusion rule against the current 105 CMR 300.000 and current DPH guidance,
   and checks the measles, infection control, and guide corpus content.
2. **Acceptance script (Part B).** The nurse runs realistic prompts in Claude Desktop and records
   pass/fail.

Run Part B with the synthetic corpus (`fixtures/synthetic/data`) mounted as `/data` and the `cd`
toolset enabled (for example `PHN_TOOLSETS=core,cd,epi`). `church_supper_outbreak.csv` is a
fictional outbreak (60 attendees, 34 ill, stuffing is the vehicle); `line_list.csv` is a
fictional case list full of seeded identifiers that must never appear in an answer.

## Part A: content review checklist

How to review: open each citation (105 CMR 300.200 in the Trial Court Law Libraries copy dated
Mass. Register #1470, 5/27/22, and the DPH guide chapters linked in
`content/cd/disease_rules.yaml` under `sources`). Check the wording, the day counting (day 0 is
the anchor; "through day N" means back on day N + 1), the settings, and the conditions ("When").
Write your initials and the date in the last column, or a correction. When everything is
checked, set `meta.review` in each file (`status: reviewed`, `reviewed_by`, `reviewed_on`, the
next `review_due`), bump `version`, and remove resolved `confidence: low` markers (see
CONTRIBUTING.md). Items to confirm first:

- [ ] 105 CMR 300.000 has not been amended since Mass. Register #1470 (5/27/22).
- [ ] Respiratory virus rules (influenza, COVID-19, RSV) match DPH's current guidance for the
      public and for health care personnel (the HCP guidance in the content is effective
      9/1/2025).
- [ ] The measles content (2013 ACIP statement, DPH 2022 chapter) is current, including immune
      globulin products and doses.
- [ ] The rules marked **medium** confidence below (26 rules: DPH guide setting guidance for
      child care and school, and fact-sheet based rules for impetigo, conjunctivitis, scabies,
      MRSA, and hand, foot, and mouth disease) are acceptable as written.
- [ ] Tuberculosis stays "refer" (not computed).
- [ ] The DPH guide corpus (`snapshots/ma_dph_guide_corpus.txt`) is acceptable as a reference
      even though most chapters are dated 2016–2018; the Streptococcus pneumoniae chapter (a
      legacy .doc file) is missing.
- [ ] Infection control content (`content/cd/infection_control.yaml`): precautions and durations
      match CDC/HICPAC Appendix A; setting notes are appropriate for Massachusetts shelters, day
      programs, schools, child care, and long-term care; EPA list assignments are right.

### Disease rules (`content/cd/disease_rules.yaml`)

"When" lists the facts that decide whether a rule applies. Anchors: onset (the disease's onset
event), first/last exposure, case onset, last case onset, positive specimen, treatment start,
symptom resolution.

| # | Disease (confidence) | Rule | Role | Settings | When | Start | End | Citation | Verbatim | Confidence | Verified (initials, date) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | Pertussis (high) | case_all | case | all |  |  | through onset +21; or 5 days of treatment | 105 CMR 300.200(A), Pertussis, Minimum Period of Isolation of Patient | yes | high |   |
| 2 | Pertussis (high) | contact_symptomatic | contact | all | symptomatic=yes |  | through onset +21; or 5 days of treatment | 105 CMR 300.200(A), Pertussis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 3 | Pertussis (high) | contact_hcw_no_pep | contact | health care worker | symptomatic=no, received prophylaxis=no | last exposure +0 | through last exposure +21 | 105 CMR 300.200(A), Pertussis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 4 | Pertussis (high) | contact_hcw_pep | contact | health care worker | symptomatic=no, received prophylaxis=yes |  | — | DPH Guide, Pertussis (2022), Section 4A | yes | high |   |
| 5 | Pertussis (high) | contact_asymptomatic_other | contact | child care, school, food handler, other | symptomatic=no |  | — | 105 CMR 300.200(A), Pertussis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 6 | Measles (rubeola) (high) | case_all | case | all |  |  | through onset +4 | 105 CMR 300.200(A), Measles, Minimum Period of Isolation of Patient | yes | high |   |
| 7 | Measles (rubeola) (high) | contact_susceptible | contact | child care, school, food handler, other | susceptible=yes | first exposure +5 | through last exposure +21 | 105 CMR 300.200(A), Measles, Minimum Period of Quarantine of Contacts | yes | high |   |
| 8 | Measles (rubeola) (high) | contact_susceptible_continuous | contact | child care, school, food handler, other | susceptible=yes, exposure continuous=yes | first exposure +5 | through last case onset +21 | 105 CMR 300.200(A), Measles, Minimum Period of Quarantine of Contacts | yes | high |   |
| 9 | Measles (rubeola) (high) | contact_hcw | contact | health care worker | susceptible=yes | first exposure +5 | through last exposure +21 | 105 CMR 300.200(A), Measles, Minimum Period of Quarantine of Contacts | yes | high |   |
| 10 | Measles (rubeola) (high) | contact_immune | contact | all | susceptible=no |  | — | DPH Guide, Measles (2022), Section 4B-4C | no | high |   |
| 11 | Mumps (high) | case_all | case | all |  |  | through onset +5 | 105 CMR 300.200(A), Mumps, Minimum Period of Isolation of Patient | yes | high |   |
| 12 | Mumps (high) | contact_susceptible | contact | child care, school, food handler, other | susceptible=yes | first exposure +12 | through last exposure +25 | 105 CMR 300.200(A), Mumps, Minimum Period of Quarantine of Contacts | yes | high |   |
| 13 | Mumps (high) | contact_susceptible_multiple | contact | child care, school, food handler, other | susceptible=yes, exposure continuous=yes | first exposure +12 | through last case onset +25 | 105 CMR 300.200(A), Mumps, Minimum Period of Quarantine of Contacts | yes | high |   |
| 14 | Mumps (high) | contact_hcw | contact | health care worker | susceptible=yes | first exposure +12 | through last exposure +25 | 105 CMR 300.200(A), Mumps, Minimum Period of Quarantine of Contacts | yes | high |   |
| 15 | Rubella (non-congenital) (high) | case_all | case | all |  |  | through onset +7 | 105 CMR 300.200(A), Rubella (b) Non-Congenital, Minimum Period of Isolation of Patient | yes | high |   |
| 16 | Rubella (non-congenital) (high) | contact_susceptible | contact | child care, school, food handler, other | susceptible=yes | last exposure +7 | through last exposure +23 | 105 CMR 300.200(A), Rubella (b) Non-Congenital, Minimum Period of Quarantine of Contacts | yes | high |   |
| 17 | Rubella (non-congenital) (high) | contact_susceptible_multiple | contact | child care, school, food handler, other | susceptible=yes, exposure continuous=yes | last exposure +7 | through last case onset +23 | 105 CMR 300.200(A), Rubella (b) Non-Congenital, Minimum Period of Quarantine of Contacts | yes | high |   |
| 18 | Rubella (non-congenital) (high) | contact_hcw | contact | health care worker | susceptible=yes | first exposure +7 | through last exposure +23 | 105 CMR 300.200(A), Rubella (b) Non-Congenital, Minimum Period of Quarantine of Contacts | yes | high |   |
| 19 | Varicella (chickenpox) (high) | case_vesicles | case | all | lesions present=yes |  | typically onset +5; criteria: All lesions have dried and crusted, or no new lesions appear | 105 CMR 300.200(A), Varicella (chickenpox), Minimum Period of Isolation of Patient | yes | high |   |
| 20 | Varicella (chickenpox) (high) | case_no_vesicles | case | all | lesions present=no |  | criteria: Lesions have faded (are resolving), or no new lesions appear within a 24-hour period, whichever is later | 105 CMR 300.200(A), Varicella (chickenpox), Minimum Period of Isolation of Patient | yes | high |   |
| 21 | Varicella (chickenpox) (high) | contact_susceptible | contact | child care, school, food handler, other | susceptible=yes | first exposure +8 | through last exposure +21 | 105 CMR 300.200(A), Varicella (chickenpox), Minimum Period of Quarantine of Contacts | yes | high |   |
| 22 | Varicella (chickenpox) (high) | contact_susceptible_continuous | contact | child care, school, food handler, other | susceptible=yes, exposure continuous=yes | case onset +8 | through case onset +21 | 105 CMR 300.200(A), Varicella (chickenpox), Minimum Period of Quarantine of Contacts | yes | high |   |
| 23 | Varicella (chickenpox) (high) | contact_hcw | contact | health care worker | susceptible=yes | first exposure +8 | through last exposure +21 | 105 CMR 300.200(A), Varicella (chickenpox), Minimum Period of Quarantine of Contacts | yes | high |   |
| 24 | Varicella (chickenpox) (high) | contact_ig | contact | all | susceptible=yes, received immune globulin=yes | first exposure +8 | through last exposure +28 | 105 CMR 300.200(A), Varicella (chickenpox), Minimum Period of Quarantine of Contacts | yes | high |   |
| 25 | Haemophilus influenzae type b (Hib), invasive (high) | case_all | case | all |  |  | 24 h after treatment start | 105 CMR 300.200(A), Haemophilus influenzae, invasive infection (a) type B, Minimum Period of Isolation of Patient | yes | high |   |
| 26 | Haemophilus influenzae type b (Hib), invasive (high) | contact_all | contact | all |  |  | — | 105 CMR 300.200(A), Haemophilus influenzae, invasive infection (a) type B, Minimum Period of Quarantine of Contacts | yes | high |   |
| 27 | Meningococcal disease, invasive (high) | case_all | case | all |  |  | 24 h after treatment start | 105 CMR 300.200(A), Meningococcal disease, invasive infection, Minimum Period of Isolation of Patient | yes | high |   |
| 28 | Meningococcal disease, invasive (high) | contact_all | contact | all |  |  | — | 105 CMR 300.200(A), Meningococcal disease, invasive infection, Minimum Period of Quarantine of Contacts | yes | high |   |
| 29 | Salmonellosis (non-typhoidal) (high) | case_food_handler | case | food handler | outbreak=no |  | 1 neg. stool(s); criteria: Diarrhea has resolved | 105 CMR 300.200(A), Salmonellosis (a) Not including typhoid fever, Minimum Period of Isolation of Patient | yes | high |   |
| 30 | Salmonellosis (non-typhoidal) (high) | case_food_handler_outbreak | case | food handler | outbreak=yes |  | 2 neg. stool(s) 24 h apart; criteria: Diarrhea has resolved | 105 CMR 300.200(A), Salmonellosis (a) Not including typhoid fever, Minimum Period of Isolation of Patient | yes | high |   |
| 31 | Salmonellosis (non-typhoidal) (high) | case_child_care | case | child care | diarrhea=yes |  | criteria: Diarrhea has resolved | DPH Guide, Salmonellosis (Non-Typhoid), Section 4C, Daycare | yes | medium |   |
| 32 | Salmonellosis (non-typhoidal) (high) | case_school | case | school | diarrhea=yes |  | criteria: Diarrhea has resolved | DPH Guide, Salmonellosis (Non-Typhoid), Section 4C, School | yes | medium |   |
| 33 | Salmonellosis (non-typhoidal) (high) | case_hcw | case | health care worker |  |  | 1 neg. stool(s); criteria: Diarrhea has resolved | DPH Guide, Salmonellosis (Non-Typhoid), Section 4C, Community Residential Programs | yes | medium |   |
| 34 | Salmonellosis (non-typhoidal) (high) | case_other | case | other |  |  | — | 105 CMR 300.200(A), Salmonellosis (a) | no | high |   |
| 35 | Salmonellosis (non-typhoidal) (high) | contact_food_handler_symptomatic | contact | food handler | diarrhea=yes |  | 1 neg. stool(s); criteria: Same as a case: diarrhea resolved and negative stool specimen(s) | 105 CMR 300.200(A), Salmonellosis (a), Minimum Period of Quarantine of Contacts | yes | high |   |
| 36 | Salmonellosis (non-typhoidal) (high) | contact_food_handler_outbreak | contact | food handler | diarrhea=no, outbreak=yes |  | 2 neg. stool(s) 24 h apart | 105 CMR 300.200(A), Salmonellosis (a), Minimum Period of Quarantine of Contacts | yes | high |   |
| 37 | Typhoid fever (Salmonella Typhi) (high) | case_food_handler | case | food handler |  |  | 3 neg. stool(s) 48 h apart | 105 CMR 300.200(A), Salmonellosis (b) S. typhi (typhoid fever), Minimum Period of Isolation of Patient | yes | high |   |
| 38 | Typhoid fever (Salmonella Typhi) (high) | contact_food_handler | contact | food handler |  |  | 3 neg. stool(s) 48 h apart | 105 CMR 300.200(A), Salmonellosis (b) S. typhi, Minimum Period of Quarantine of Contacts | yes | high |   |
| 39 | Typhoid fever (Salmonella Typhi) (high) | case_other | case | child care, school, health care worker, other |  |  | — | 105 CMR 300.200(A), Salmonellosis (b) | no | medium |   |
| 40 | Shigellosis (high) | case_food_handler | case | food handler, health care worker |  |  | 2 neg. stool(s) 24 h apart; criteria: Diarrhea has resolved | 105 CMR 300.200(A), Shigellosis, Minimum Period of Isolation of Patient | yes | high |   |
| 41 | Shigellosis (high) | case_child_care | case | child care |  |  | 2 neg. stool(s) 24 h apart; criteria: Diarrhea has resolved | DPH Guide, Shigellosis (2018), Section 4C, Daycare | yes | medium |   |
| 42 | Shigellosis (high) | case_school | case | school | diarrhea=yes |  | criteria: Diarrhea has resolved | DPH Guide, Shigellosis (2018), Section 4C, School | yes | medium |   |
| 43 | Shigellosis (high) | case_other | case | other |  |  | — | 105 CMR 300.200(A), Shigellosis | no | high |   |
| 44 | Shigellosis (high) | contact_food_handler_symptomatic | contact | food handler | diarrhea=yes |  | 2 neg. stool(s) 24 h apart; criteria: Same as a case | 105 CMR 300.200(A), Shigellosis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 45 | Shigellosis (high) | contact_food_handler_outbreak | contact | food handler | diarrhea=no, outbreak=yes |  | 2 neg. stool(s) 24 h apart | 105 CMR 300.200(A), Shigellosis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 46 | Shiga toxin-producing E. coli (STEC), including E. coli O157:H7 (high) | case_food_handler | case | food handler, health care worker |  |  | 2 neg. stool(s) 24 h apart; criteria: Diarrhea has resolved | 105 CMR 300.200(A), Shiga toxin-producing organisms, including E. coli O157:H7, Minimum Period of Isolation of Patient | yes | high |   |
| 47 | Shiga toxin-producing E. coli (STEC), including E. coli O157:H7 (high) | case_child_care | case | child care |  |  | 2 neg. stool(s) 24 h apart; criteria: Diarrhea has resolved | DPH Guide, Shiga Toxin-Producing E. coli (2018), Section 4C, Daycare | yes | medium |   |
| 48 | Shiga toxin-producing E. coli (STEC), including E. coli O157:H7 (high) | case_school | case | school | diarrhea=yes |  | criteria: Diarrhea has resolved | DPH Guide, Shiga Toxin-Producing E. coli (2018), Section 4C, School | yes | medium |   |
| 49 | Shiga toxin-producing E. coli (STEC), including E. coli O157:H7 (high) | case_other | case | other |  |  | — | 105 CMR 300.200(A), Shiga toxin-producing organisms | no | high |   |
| 50 | Shiga toxin-producing E. coli (STEC), including E. coli O157:H7 (high) | contact_food_handler_symptomatic | contact | food handler | diarrhea=yes |  | 2 neg. stool(s) 24 h apart; criteria: Same as a case | 105 CMR 300.200(A), Shiga toxin-producing organisms, Minimum Period of Quarantine of Contacts | yes | high |   |
| 51 | Shiga toxin-producing E. coli (STEC), including E. coli O157:H7 (high) | contact_food_handler_outbreak | contact | food handler | diarrhea=no, outbreak=yes |  | 2 neg. stool(s) 24 h apart | 105 CMR 300.200(A), Shiga toxin-producing organisms, Minimum Period of Quarantine of Contacts | yes | high |   |
| 52 | Campylobacteriosis (high) | case_food_handler | case | food handler, health care worker | outbreak=no |  | 1 neg. stool(s); criteria: Diarrhea has resolved | 105 CMR 300.200(A), Campylobacteriosis, Minimum Period of Isolation of Patient | yes | high |   |
| 53 | Campylobacteriosis (high) | case_food_handler_outbreak | case | food handler, health care worker | outbreak=yes |  | 2 neg. stool(s) 24 h apart; criteria: Diarrhea has resolved | 105 CMR 300.200(A), Campylobacteriosis, Minimum Period of Isolation of Patient | yes | high |   |
| 54 | Campylobacteriosis (high) | case_child_care | case | child care | diarrhea=yes |  | criteria: Diarrhea has resolved | DPH Guide, Campylobacter Enteritis (2016), Section 4C, Daycare | yes | medium |   |
| 55 | Campylobacteriosis (high) | case_school | case | school | diarrhea=yes |  | criteria: Diarrhea has resolved | DPH Guide, Campylobacter Enteritis (2016), Section 4C, School | yes | medium |   |
| 56 | Campylobacteriosis (high) | case_other | case | other |  |  | — | 105 CMR 300.200(A), Campylobacteriosis | no | high |   |
| 57 | Campylobacteriosis (high) | contact_food_handler_symptomatic | contact | food handler | diarrhea=yes |  | 1 neg. stool(s); criteria: Same as a case | 105 CMR 300.200(A), Campylobacteriosis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 58 | Campylobacteriosis (high) | contact_food_handler_outbreak | contact | food handler | diarrhea=no, outbreak=yes |  | 2 neg. stool(s) 24 h apart | 105 CMR 300.200(A), Campylobacteriosis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 59 | Giardiasis (high) | case_food_handler | case | food handler, health care worker | outbreak=no |  | 1 neg. stool(s); criteria: Diarrhea has resolved | 105 CMR 300.200(A), Giardiasis, Minimum Period of Isolation of Patient | yes | high |   |
| 60 | Giardiasis (high) | case_food_handler_outbreak | case | food handler, health care worker | outbreak=yes |  | 2 neg. stool(s) 24 h apart; criteria: Diarrhea has resolved | 105 CMR 300.200(A), Giardiasis, Minimum Period of Isolation of Patient | yes | high |   |
| 61 | Giardiasis (high) | case_child_care | case | child care | diarrhea=yes |  | criteria: Diarrhea has resolved | DPH Guide, Giardiasis (2016), Section 4C, Daycare | yes | medium |   |
| 62 | Giardiasis (high) | case_school | case | school | diarrhea=yes |  | criteria: Diarrhea has resolved | DPH Guide, Giardiasis (2016), Section 4C, School | yes | medium |   |
| 63 | Giardiasis (high) | case_other | case | other |  |  | — | 105 CMR 300.200(A), Giardiasis | no | high |   |
| 64 | Giardiasis (high) | contact_food_handler_symptomatic | contact | food handler | diarrhea=yes |  | 1 neg. stool(s); criteria: Same as a case | 105 CMR 300.200(A), Giardiasis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 65 | Giardiasis (high) | contact_food_handler_outbreak | contact | food handler | diarrhea=no, outbreak=yes |  | 2 neg. stool(s) 24 h apart | 105 CMR 300.200(A), Giardiasis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 66 | Cryptosporidiosis (high) | case_food_handler | case | food handler, health care worker | outbreak=no |  | 1 neg. stool(s); criteria: Diarrhea has resolved | 105 CMR 300.200(A), Cryptosporidiosis, Minimum Period of Isolation of Patient | yes | high |   |
| 67 | Cryptosporidiosis (high) | case_food_handler_outbreak | case | food handler, health care worker | outbreak=yes |  | 2 neg. stool(s) 24 h apart; criteria: Diarrhea has resolved | 105 CMR 300.200(A), Cryptosporidiosis, Minimum Period of Isolation of Patient | yes | high |   |
| 68 | Cryptosporidiosis (high) | case_child_care | case | child care | diarrhea=yes |  | criteria: Diarrhea has resolved | DPH Guide, Cryptosporidiosis (2016), Section 4C, Daycare | yes | medium |   |
| 69 | Cryptosporidiosis (high) | case_school | case | school | diarrhea=yes |  | criteria: Diarrhea has resolved | DPH Guide, Cryptosporidiosis (2016), Section 4C, School | yes | medium |   |
| 70 | Cryptosporidiosis (high) | case_other | case | other |  |  | — | 105 CMR 300.200(A), Cryptosporidiosis | no | medium |   |
| 71 | Cryptosporidiosis (high) | contact_food_handler_symptomatic | contact | food handler | diarrhea=yes |  | 1 neg. stool(s); criteria: Same as a case | 105 CMR 300.200(A), Cryptosporidiosis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 72 | Cryptosporidiosis (high) | contact_food_handler_outbreak | contact | food handler | diarrhea=no, outbreak=yes |  | 2 neg. stool(s) 24 h apart | 105 CMR 300.200(A), Cryptosporidiosis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 73 | Cyclosporiasis (high) | case_food_handler | case | food handler, health care worker |  |  | criteria: Diarrhea has resolved; In certain situations, one or two negative stool specimens | 105 CMR 300.200(A), Cyclosporiasis, Minimum Period of Isolation of Patient | yes | high |   |
| 74 | Cyclosporiasis (high) | case_child_care_school | case | child care, school |  |  | criteria: No diarrhea and feels well enough to attend | DPH Guide, Cyclosporiasis (2016), Section 4C, Daycare and Schools | yes | medium |   |
| 75 | Cyclosporiasis (high) | case_other | case | other |  |  | — | 105 CMR 300.200(A), Cyclosporiasis | no | high |   |
| 76 | Cyclosporiasis (high) | contact_food_handler_symptomatic | contact | food handler | diarrhea=yes |  | criteria: Same as a case | 105 CMR 300.200(A), Cyclosporiasis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 77 | Yersiniosis (high) | case_food_handler | case | food handler, health care worker | outbreak=no |  | 1 neg. stool(s); criteria: Diarrhea has resolved | 105 CMR 300.200(A), Yersiniosis, Minimum Period of Isolation of Patient | yes | high |   |
| 78 | Yersiniosis (high) | case_food_handler_outbreak | case | food handler, health care worker | outbreak=yes |  | 2 neg. stool(s) 24 h apart; criteria: Diarrhea has resolved | 105 CMR 300.200(A), Yersiniosis, Minimum Period of Isolation of Patient | yes | high |   |
| 79 | Yersiniosis (high) | case_other | case | child care, school, other |  |  | — | 105 CMR 300.200(A), Yersiniosis | no | medium |   |
| 80 | Yersiniosis (high) | contact_food_handler_symptomatic | contact | food handler | diarrhea=yes |  | 1 neg. stool(s); criteria: Same as a case | 105 CMR 300.200(A), Yersiniosis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 81 | Yersiniosis (high) | contact_food_handler_outbreak | contact | food handler | diarrhea=no, outbreak=yes |  | 2 neg. stool(s) 24 h apart | 105 CMR 300.200(A), Yersiniosis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 82 | Vibriosis (non-cholera) (high) | case_food_handler | case | food handler |  |  | criteria: Diarrhea has resolved | 105 CMR 300.200(A), Vibriosis (non-Cholera), Minimum Period of Isolation of Patient | yes | high |   |
| 83 | Vibriosis (non-cholera) (high) | case_other | case | child care, school, health care worker, other |  |  | — | 105 CMR 300.200(A), Vibriosis (non-Cholera) | no | high |   |
| 84 | Amebiasis (high) | case_food_handler | case | food handler, health care worker | outbreak=no |  | 1 neg. stool(s); criteria: Diarrhea has resolved | 105 CMR 300.200(A), Amebiasis, Minimum Period of Isolation of Patient | yes | high |   |
| 85 | Amebiasis (high) | case_food_handler_outbreak | case | food handler, health care worker | outbreak=yes |  | 2 neg. stool(s) 24 h apart; criteria: Diarrhea has resolved | 105 CMR 300.200(A), Amebiasis, Minimum Period of Isolation of Patient | yes | high |   |
| 86 | Amebiasis (high) | case_other | case | child care, school, other |  |  | — | 105 CMR 300.200(A), Amebiasis | no | medium |   |
| 87 | Amebiasis (high) | contact_food_handler_symptomatic | contact | food handler | diarrhea=yes |  | 1 neg. stool(s); criteria: Same as a case | 105 CMR 300.200(A), Amebiasis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 88 | Amebiasis (high) | contact_food_handler_outbreak | contact | food handler | diarrhea=no, outbreak=yes |  | 2 neg. stool(s) 24 h apart | 105 CMR 300.200(A), Amebiasis, Minimum Period of Quarantine of Contacts | yes | high |   |
| 89 | Hepatitis A (high) | case_all | case | all |  |  | through onset +7 | 105 CMR 300.200(A), Hepatitis A, Minimum Period of Isolation of Patient | yes | medium |   |
| 90 | Hepatitis A (high) | contact_food_handler | contact | food handler | susceptible=yes, received prophylaxis=no | last exposure +0 | through last exposure +28 | 105 CMR 300.200(A), Hepatitis A, Minimum Period of Quarantine of Contacts | yes | high |   |
| 91 | Hepatitis A (high) | contact_child_care | contact | child care |  |  | — | DPH Guide, Hepatitis A (2016), Section 4D, Daycare | yes | medium |   |
| 92 | Hepatitis A (high) | contact_other | contact | school, health care worker, other |  |  | — | 105 CMR 300.200(A), Hepatitis A; DPH Guide Section 4B | no | high |   |
| 93 | Norovirus (high) | case_food_handler | case | food handler, health care worker |  |  | 72 h after symptoms resolve; 72 h after specimen | 105 CMR 300.200(A), Noroviruses, Minimum Period of Isolation of Patient | yes | high |   |
| 94 | Norovirus (high) | case_child_care_school_children | case | child care, school |  |  | 24 h after symptoms resolve; criteria: 24 hours since the last vomiting episode; 24 hours with improvement in diarrhea (no more than 2 stools above the child's normal; stools contained in the diaper or no toileting accidents); if baseline is unknown, 24 hours after diarrhea resolved; Well enough to participate | DPH Gastrointestinal Illness Symptom Checklist for Children and Staff in Child Care Programs and K-12 Schools (V3) | no | high |   |
| 95 | Norovirus (high) | case_child_care_school_staff | case | child care, school |  |  | 24 h after symptoms resolve; criteria: 24 hours since the last vomiting episode and diarrhea has improved | DPH Gastrointestinal Illness Symptom Checklist, For Staff with GI Illness | no | high |   |
| 96 | Norovirus (high) | case_other | case | other |  |  | 24 h after symptoms resolve; criteria: 24 hours since the last vomiting episode and diarrhea has improved | DPH Norovirus fact sheet, I have norovirus. How do I avoid spreading it to others? | yes | high |   |
| 97 | Norovirus (high) | contact_food_handler_symptomatic | contact | food handler, health care worker | symptomatic=yes |  | 72 h after symptoms resolve | 105 CMR 300.200(A), Noroviruses, Minimum Period of Quarantine of Contacts | yes | high |   |
| 98 | Influenza (seasonal) (high) | case_community | case | child care, school, food handler, other |  |  | criteria: No fever for at least 24 hours without fever-reducing medicines; Other symptoms are improving; Then take extra precautions (mask indoors, avoid crowds, hand hygiene) for the next 5 days | DPH, Staying home to prevent the spread of respiratory viruses (If you have symptoms; Schools and childcare settings) | yes | high |   |
| 99 | Influenza (seasonal) (high) | case_hcw_symptomatic | case | health care worker | symptomatic=yes |  | through onset +3; criteria: Symptoms improving, including fever-free 24 hours without antipyretics; Feels well enough to work; Wear a facemask until day 8 (masks may be removed on day 8) | DPH, Guidance for Health Care Personnel with a Respiratory Viral Pathogen Infection or Exposure (effective 9/1/2025) | no | high |   |
| 100 | Influenza (seasonal) (high) | case_hcw_asymptomatic | case | health care worker | symptomatic=no |  | through positive specimen +3; criteria: Wear a facemask until day 8 after the positive test | DPH, Guidance for Health Care Personnel with a Respiratory Viral Pathogen Infection or Exposure (effective 9/1/2025) | no | high |   |
| 101 | Influenza (seasonal) (high) | contact_all | contact | all | symptomatic=no |  | — | DPH respiratory virus guidance for the public and for health care personnel | no | high |   |
| 102 | COVID-19 (high) | case_community | case | child care, school, food handler, other |  |  | criteria: No fever for at least 24 hours without fever-reducing medicines; Other symptoms are improving; Then take extra precautions (mask indoors, avoid crowds, hand hygiene) for the next 5 days | DPH, Staying home to prevent the spread of respiratory viruses (If you have symptoms; Schools and childcare settings) | yes | high |   |
| 103 | COVID-19 (high) | case_hcw_symptomatic | case | health care worker | symptomatic=yes |  | through onset +3; criteria: Symptoms improving, including fever-free 24 hours without antipyretics; Feels well enough to work; Wear a facemask until day 8 (masks may be removed on day 8) | DPH, Guidance for Health Care Personnel with a Respiratory Viral Pathogen Infection or Exposure (effective 9/1/2025) | no | high |   |
| 104 | COVID-19 (high) | case_hcw_asymptomatic | case | health care worker | symptomatic=no |  | through positive specimen +3; criteria: Wear a facemask until day 8 after the positive test | DPH, Guidance for Health Care Personnel with a Respiratory Viral Pathogen Infection or Exposure (effective 9/1/2025) | no | high |   |
| 105 | COVID-19 (high) | contact_all | contact | all | symptomatic=no |  | — | DPH respiratory virus guidance for the public and for health care personnel | no | high |   |
| 106 | Respiratory syncytial virus (RSV) (medium) | case_community | case | child care, school, food handler, other |  |  | criteria: No fever for at least 24 hours without fever-reducing medicines; Other symptoms are improving; Then take extra precautions (mask indoors, avoid crowds, hand hygiene) for the next 5 days | DPH, Staying home to prevent the spread of respiratory viruses (If you have symptoms; Schools and childcare settings) | yes | high |   |
| 107 | Respiratory syncytial virus (RSV) (medium) | case_hcw_symptomatic | case | health care worker | symptomatic=yes |  | through onset +3; criteria: Symptoms improving, including fever-free 24 hours without antipyretics; Feels well enough to work; Wear a facemask until day 8 (masks may be removed on day 8) | DPH, Guidance for Health Care Personnel with a Respiratory Viral Pathogen Infection or Exposure (effective 9/1/2025) | no | high |   |
| 108 | Respiratory syncytial virus (RSV) (medium) | case_hcw_asymptomatic | case | health care worker | symptomatic=no |  | through positive specimen +3; criteria: Wear a facemask until day 8 after the positive test | DPH, Guidance for Health Care Personnel with a Respiratory Viral Pathogen Infection or Exposure (effective 9/1/2025) | no | high |   |
| 109 | Respiratory syncytial virus (RSV) (medium) | contact_all | contact | all | symptomatic=no |  | — | DPH respiratory virus guidance for the public and for health care personnel | no | high |   |
| 110 | Group A streptococcal infection (pharyngitis, skin infection, invasive disease) (high) | case_school_child_care | case | child care, school |  |  | 24 h after treatment start | 105 CMR 300.200(A), Group A streptococcus, invasive infection, Minimum Period of Isolation of Patient | yes | high |   |
| 111 | Group A streptococcal infection (pharyngitis, skin infection, invasive disease) (high) | case_hcw_food_handler | case | health care worker, food handler |  |  | 24 h after treatment start | DPH Group A Streptococcal Disease fact sheet | yes | high |   |
| 112 | Group A streptococcal infection (pharyngitis, skin infection, invasive disease) (high) | case_other | case | other |  |  | — | 105 CMR 300.200(A), Group A streptococcus, invasive infection | no | high |   |
| 113 | Group A streptococcal infection (pharyngitis, skin infection, invasive disease) (high) | contact_all | contact | all |  |  | — | 105 CMR 300.200(A), Group A streptococcus, invasive infection, Minimum Period of Quarantine of Contacts | yes | high |   |
| 114 | Impetigo (medium) | case_school_child_care | case | child care, school, other |  |  | 24 h after treatment start; criteria: Cover all sores on exposed skin | 105 CMR 300.200(A), Group A streptococcus (skin infections); CDC Impetigo (2025) | no | medium |   |
| 115 | Impetigo (medium) | case_hcw | case | health care worker, food handler |  |  | 24 h after treatment start | CDC/HICPAC Isolation Precautions, Appendix A, Impetigo | yes | high |   |
| 116 | Conjunctivitis (pink eye), viral or bacterial (medium) | case_all | case | child care, school, other |  |  | criteria: No fever or other symptoms, with the doctor's approval; Not attending while symptomatic if activities involve close contact with others | CDC, About Pink Eye (Conjunctivitis), When to return to school or work | yes | medium |   |
| 117 | Conjunctivitis (pink eye), viral or bacterial (medium) | case_hcw | case | health care worker |  |  | criteria: Contact + Standard precautions for the duration of illness (acute viral); Standard precautions for acute bacterial | CDC/HICPAC Isolation Precautions, Appendix A, Conjunctivitis | no | high |   |
| 118 | Scabies (medium) | case_school | case | child care, school |  |  | criteria: Has been treated | DPH Scabies fact sheet, How can scabies be prevented? | yes | medium |   |
| 119 | Scabies (medium) | case_hcw | case | health care worker |  |  | 24 h after treatment start | CDC/HICPAC Isolation Precautions, Appendix A, Scabies | no | high |   |
| 120 | Scabies (medium) | contact_all | contact | all |  |  | — | DPH Scabies fact sheet | yes | medium |   |
| 121 | Head lice (pediculosis) (high) | case_school | case | child care, school, other |  |  | — | DPH Head Lice (Pediculosis) fact sheet, What is a "no nits" policy? | yes | high |   |
| 122 | MRSA skin infection (methicillin-resistant Staphylococcus aureus) (medium) | case_all | case | child care, school, food handler, other |  |  | criteria: Keep wounds clean and covered with a bandage until healed; Do not share towels, razors, or athletic equipment | DPH MRSA fact sheet, How can I prevent a staph infection? | yes | medium |   |
| 123 | MRSA skin infection (methicillin-resistant Staphylococcus aureus) (medium) | case_hcw | case | health care worker |  |  | criteria: Contact + Standard precautions as judged by the infection control program (ongoing transmission, acute care, or wounds that cannot be contained) | CDC/HICPAC Isolation Precautions, Appendix A, Multidrug-resistant organisms | no | high |   |
| 124 | Hand, foot, and mouth disease (medium) | case_school | case | child care, school |  |  | criteria: No fever; Feels well enough to participate in class | CDC, About Hand, Foot, and Mouth Disease, When to return to school | no | medium |   |
| 125 | Fifth disease (parvovirus B19) (high) | case_school | case | child care, school, other |  |  | — | DPH Fifth Disease fact sheet, Should children with fifth disease be kept home? | yes | high |   |
| 126 | Hepatitis B (high) | case_all | case | all |  |  | — | 105 CMR 300.200(A), Hepatitis B, Minimum Period of Isolation of Patient | yes | high |   |
| 127 | Hepatitis B (high) | contact_all | contact | all |  |  | — | 105 CMR 300.200(A), Hepatitis B, Minimum Period of Quarantine of Contacts | yes | high |   |
| 128 | Hepatitis C (high) | case_all | case | all |  |  | — | 105 CMR 300.200(A), Hepatitis C, Minimum Period of Isolation of Patient | yes | high |   |
| 129 | Hepatitis C (high) | contact_all | contact | all |  |  | — | 105 CMR 300.200(A), Hepatitis C, Minimum Period of Quarantine of Contacts | yes | high |   |
| 130 | Legionellosis (Legionnaires' disease, Pontiac fever) (high) | case_all | case | all |  |  | — | 105 CMR 300.200(A), Legionellosis | yes | high |   |
| 131 | Listeriosis (high) | case_all | case | all |  |  | — | 105 CMR 300.200(A), Listeriosis | yes | high |   |
| 132 | Lyme disease (high) | case_all | case | all |  |  | — | 105 CMR 300.200(A), Lyme disease | yes | high |   |
| 133 | Tuberculosis (active) (high) | case_pulmonary | case | all |  |  | criteria: Clearance requires one or more of - three appropriately collected sputum smears 8-24 hours apart (one early morning); other accepted tests indicating TB or infectiousness is unlikely; or effective chemotherapy per current standards with clinical improvement | 105 CMR 300.200(B), Tuberculosis (a) Active tuberculosis - Pulmonary | no | high |   |
| 134 | Tuberculosis (active) (high) | contact_all | contact | all |  |  | — | 105 CMR 300.200(B), Tuberculosis, Minimum Period of Quarantine of Contacts | yes | high |   |

_134 rules in 37 diseases._

**Incubation, infectious period, contact monitoring, and PEP by disease**

| Disease | Incubation | Infectious period (anchor, days) | Monitoring (days after last exposure) | PEP windows | Source | Verified |
|---|---|---|---|---|---|---|
| Pertussis | 5–21 days (usual 7–10) | onset -7..21; ends after 5 days of treatment | 21 | Antibiotic post-exposure prophylaxis within 504 h of last exposure | guide-pertussis-2022 |   |
| Measles (rubeola) | 7–21 days (usual 11–14) | onset -4..4 | 21 | MMR vaccine within 72 h of first exposure; Immune globulin within 144 h of first exposure | guide-measles-2022 |   |
| Mumps | 12–25 days (usual 16–18) | onset -2..5 | 25 | — | guide-mumps-2022 |   |
| Rubella (non-congenital) | 12–23 days (usual 17–17) | onset -7..7 | 23 | — | cmr300, guide-rubella-and-crs-2022 |   |
| Varicella (chickenpox) | 10–21 days (usual 14–16) | onset -2..5 | 21 | Varicella vaccine within 120 h of first exposure; VariZIG within 240 h of first exposure | cmr300, guide-chickenpox-2023 |   |
| Haemophilus influenzae type b (Hib), invasive | not stated | not stated | — | — |  |   |
| Meningococcal disease, invasive | 1–10 days (usual None–4) | onset -7..None | 14 | — | guide-invasive-meningococcal-disease-2018 |   |
| Salmonellosis (non-typhoidal) | 6–72 hours (usual 12–36) | text only | — | — | guide-salmonellosis-non-typhoid |   |
| Typhoid fever (Salmonella Typhi) | 6–30 days (usual None–None) | not stated | — | — | cdc-typhoid-hcp |   |
| Shigellosis | 12–96 hours (usual 24–72) | text only | — | — | guide-shigellosis-2018 |   |
| Shiga toxin-producing E. coli (STEC), including E. coli O157:H7 | 1–10 days (usual 3–4) | text only | — | — | guide-shiga-toxin-producing-escherichia-coli-2018 |   |
| Campylobacteriosis | 1–10 days (usual 2–5) | text only | — | — | guide-campylobacter-enteritis-2016 |   |
| Giardiasis | 3–25 days (usual 7–10) | text only | — | — | guide-giardiasis-2016 |   |
| Cryptosporidiosis | 2–10 days (usual 7–7) | text only | — | — | guide-cryptosporidiosis-2016 |   |
| Cyclosporiasis | 2–14 days (usual 7–7) | text only | — | — | guide-cyclosporiasis-2016 |   |
| Yersiniosis | None–10 days (usual 3–7) | text only | — | — | guide-yersiniosis-2016 |   |
| Vibriosis (non-cholera) | 5–92 hours (usual 24–24) | text only | — | — | guide-vibriosis-2018 |   |
| Amebiasis | 2–None days (usual 14–28) | text only | — | — | guide-amebiasis-2016 |   |
| Hepatitis A | 15–50 days (usual 28–28) | onset -14..7 | 50 | Hepatitis A vaccine or immune globulin within 336 h of last exposure | guide-hepatitis-a-2016 |   |
| Norovirus | 12–48 hours (usual None–None) | onset 0..None | — | — | factsheet-norovirus |   |
| Influenza (seasonal) | 1–4 days (usual 2–2) | onset -1..7 | — | — | cdc-flu-about, factsheet-flu |   |
| COVID-19 | 2–14 days (usual None–None) | onset -2..10 | — | — | dph-respiratory-public, factsheet-covid-19 |   |
| Respiratory syncytial virus (RSV) | not stated | not stated | — | — |  |   |
| Group A streptococcal infection (pharyngitis, skin infection, invasive disease) | 1–5 days (usual None–None) | onset None..None | — | — | guide-group-a-streptococcus-invasive-2018 |   |
| Impetigo | None–None days (usual 10–10) | not stated | — | — | cdc-impetigo-hcp |   |
| Conjunctivitis (pink eye), viral or bacterial | not stated | not stated | — | — |  |   |
| Scabies | 14–42 days (usual None–None) | text only | — | — | factsheet-scabies |   |
| Head lice (pediculosis) | not stated | text only | — | — | factsheet-head-lice |   |
| MRSA skin infection (methicillin-resistant Staphylococcus aureus) | not stated | not stated | — | — |  |   |
| Hand, foot, and mouth disease | not stated | not stated | — | — |  |   |
| Fifth disease (parvovirus B19) | not stated | text only | — | — | factsheet-fifth-disease |   |
| Hepatitis B | 60–150 days (usual 90–90) | not stated | — | — | cdc-hepatitis-b-hcp |   |
| Hepatitis C | 42–180 days (usual None–None) | not stated | — | — | factsheet-hepatitis-c |   |
| Legionellosis (Legionnaires' disease, Pontiac fever) | 2–10 days (usual 5–6) | text only | — | — | guide-legionellosis-2016 |   |
| Listeriosis | 3–70 days (usual 21–21) | text only | — | — | guide-listeriosis-2018 |   |
| Lyme disease | 3–30 days (usual None–None) | text only | — | — | factsheet-lyme-disease |   |
| Tuberculosis (active) | not stated | not stated | — | — |  |   |

## Part B: acceptance script

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "What does the DPH guide say about excluding food handlers with salmonellosis?" | `search_ma_disease_control_guide` | Quoted passage from the Salmonellosis chapter, Section 4; file date and mass.gov URL; decision-support notice. |
| 2 | "Show me the outline of the DPH pertussis chapter." | `search_ma_disease_control_guide` | Section list for Pertussis (2022). |
| 3 | "A 2nd grader's cough started Jan 1 and azithromycin started Jan 5. When can she go back to school?" | `calculate_isolation_and_exclusion` | Back on Jan 10 (5 full days of treatment; day 1 = Jan 5), not Jan 23; 105 CMR 300.200 wording quoted; pending-review caveat. |
| 4 | "An unvaccinated child-care worker was exposed to measles on March 2. When is she excluded?" | `calculate_isolation_and_exclusion` or `assess_measles_exposure` | March 7 through March 23; back March 24; MMR deadline March 5; IG deadline March 8. |
| 5 | "A cook at a restaurant has Shigella. What does she need to return to food handling?" | `calculate_isolation_and_exclusion` | Food handler rule: diarrhea resolved and two negative stool specimens at least 24 hours apart (the first at least 48 hours after antibiotics), quoted from 105 CMR 300.200. |
| 6 | "Norovirus in a nursing home: precautions, how long, and what disinfectant?" | `infection_control_for_congregate_settings` | Contact + Standard; 48 hours after symptoms resolve (72 in DPH's outbreak guidance); EPA List G link; cohorting; HCFRS reporting note. |
| 7 | "Head lice in a 3rd-grade class — what do we do?" | `infection_control_for_congregate_settings` | Not a health care setting; fact sheet guidance; no exclusion invented beyond the content. |
| 8 | "Analyze church_supper_outbreak.csv. The supper was May 2 at 6 pm; we suspect Salmonella." | `analyze_outbreak_line_list` | Stuffing is the likely vehicle (RR about 10.7, p < 0.001, 94% of cases ate it); exposure window includes May 2 18:00; no names, phone numbers, or IDs in the answer; small cells shown as `*` or `<5`. |
| 9 | "Only count confirmed cases." | `analyze_outbreak_line_list` | Fewer cases; probable cases are left out (not counted as non-cases). |
| 10 | "Draw the epidemic curve, by case status." | `generate_epi_curve` | 6-hour intervals (1/4 of the 24-hour typical incubation); exposure marker and incubation band; "point source"; PNG in output/charts. |
| 11 | "Put the epi curve and the attack-rate table into a Word report." | `export_document` | Image with alt text; accessibility check passes. |
| 12 | "Analyze line_list.csv as an outbreak, by town." | `analyze_outbreak_line_list` | Case series (no attack rates); towns with small counts as `*`; no names, addresses, SSNs, MRNs, or case IDs anywhere in the answer. |
| 13 | "Use the Last Name column as the town column." | `analyze_outbreak_line_list` | Refused (identifier column). |
| 14 | "Plan contact tracing for a measles case with rash onset April 10." | `plan_contact_tracing` | Infectious April 6–14; contact dates (exclusion day 5–21, MMR 72 h, IG 6 days, monitoring 21 days); blank workbook in /data/output with formulas; no request for contact names. |
| 15 | "Weekly ILI ED visits for 3 years: [counts]. Anything unusual this week?" | `detect_aberrations` | EARS, CUSUM, and Farrington results for the latest week; interpretation; trade-offs explained. |
| 16 | "Here are 10 weeks of Campylobacter counts: 1, 0, 2, 1, 0, 1, 1, 0, 2, 6." | `detect_aberrations` | Sparse-count caveat and SD floor; Farrington skipped (too little history) with the reason. |
| 17 | Use the prompt **Investigate an outbreak** (disease: norovirus; setting: long-term care). | prompt, then several tools | The model works through the steps in order, starting with reporting (DPH Epidemiology 617-983-6800) and the guide, and never asks for identifiers. |

## Part C: exit criteria for Pilot 1

- [ ] Part A complete: every disease rule verified or corrected, and `meta.review` updated in the
      four Phase 3 content files.
- [ ] Part B passes on Windows and macOS.
- [ ] The nurse is comfortable that small-cell suppression in outbreak tables (and the stated
      limits of suppression in the exposure table, where ratios are shown) meets agency policy.
