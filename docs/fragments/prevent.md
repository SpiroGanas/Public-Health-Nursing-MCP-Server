## Changelog

- `preventive_services_lookup` (CHR-04): the USPSTF grade A and B recommendations that fit a
  person's age, sex at birth, pregnancy status (pregnant, postpartum, or not), and risk factors
  (tobacco use and pack-years, BMI or BMI percentile, cardiovascular risk factors and 10-year
  risk, sexually active, postmenopausal, fair skin, low-fluoride home water, and areas where the
  clinician judges increased risk). Each result shows the grade, the date of the Task Force
  statement and a DOI link, the Task Force's own population and recommendation wording, what the
  tool could not check ("confirm"), a plain-language client summary, and related Massachusetts
  programs; a recommendation that depends on a risk factor that was not given goes under "needs
  review" with the missing input named, never dropped. Massachusetts options from DPH pages: the
  Massachusetts Breast and Cervical Cancer Program (eligibility as DPH lists it, sites, toll-free
  number), the MA Colorectal Cancer Control Program, and 1-800-QUIT-NOW. When online it adds
  ODPHP's MyHealthfinder list (live, no key) for the same age, sex, and pregnancy. Bundled
  snapshot `uspstf-ab-recommendations` (53 recommendations from 46 statements, 2016 on), built by
  `xtask/scripts/build_prevent_snapshots.py` from the Task Force's published statements read as
  PubMed abstracts (the USPSTF site and API were blocked); the builder refuses to write a grade
  that the text does not show. Optional plain-language handout (no identifiers) to
  `/data/output`.
- `asthma_home_assessment` (CHR-05): the CDC, EPA, and HUD home asthma trigger checklist for home
  visitors (core assessment plus dust mite and mold and moisture modules, with action steps),
  pointing at the 105 CMR 410 sections already in the Phase 14 housing content (moisture and
  mold, pests, heat, smoke and CO alarms) and reading them at run time instead of repeating them;
  DPH's asthma control table (adapted from NHLBI EPR-3, screening level) with a classifier for the
  counts you give (most severe item decides); the NHLBI asthma action plan template with peak flow
  zones worked out from a best peak flow; and DPH's Massachusetts Asthma Home Visiting Program
  Directory (seven programs, dated, regions served) with each program marked as naming or located
  in your town. The Asthma Control Test is licensed and is not reproduced. Writes the blank
  checklist and action plan to `/data/output`; fills action plans for the people in a local
  CSV/XLSX into `/data/output` only (medicines and doses appear only if they are in the file).

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "What USPSTF A and B screenings apply to a 52-year-old woman who smoked 25 pack-years and quit 8 years ago? BMI is 31." | `preventive_services_lookup` | Lung cancer screening (B, 2021-03-09, 50 to 80 years, 20 pack-years, quit within 15 years), colorectal 50 to 75 (A, 2021-05-18), breast (B, 2024-06-11), cervical 30 to 65 (A, 2018-08-21), hypertension, hepatitis C, HIV, prediabetes and weight management are under "applies"; statins are under "needs review" asking for the 10-year risk. Every item has a grade, a date, the Task Force's wording, and a client summary. The first caveats say this is the A and B list only and that the Task Force's site could not be reached. |
| 2 | "A 29-year-old woman is 20 weeks pregnant and smokes. Which recommended preventive services should I cover? Give me client summaries." | `preventive_services_lookup` (`pregnancy_status` pregnant) | Hepatitis B, syphilis, HIV, gestational diabetes (at 24 weeks or later: check the "confirm" note), hypertensive disorders, healthy weight gain, breastfeeding support, folic acid, and tobacco cessation in pregnancy (behavioral support; not the medicine) appear; recommendations for nonpregnant adults do not. 1-800-QUIT-NOW is listed. |
| 3 | "My client is 47, has no health insurance, and needs a mammogram and a Pap test. What can she use in Massachusetts?" | `preventive_services_lookup` | Breast (40 to 74) and cervical (30 to 65) recommendations with the Massachusetts Breast and Cervical Cancer Program: eligibility as DPH lists it (no insurance or not covering the full cost, resident, income at or below 250% FPL, ages 40 to 64 and 30 to 64), toll-free number (877) 414-4447, and the seven sites. Compare with the DPH page. The note that the program's ages differ from USPSTF's appears. |
| 4 | "What preventive services is a 70-year-old man due for?" (no risk factors) | `preventive_services_lookup` | Lung and AAA screening and fall-prevention exercise are under "needs review" with the missing input named; telling the tool he never smoked removes lung and AAA screening but not falls. Nothing is dropped silently. |
| 5 | "Make a client handout for a 47-year-old woman who has never smoked." | `preventive_services_lookup` (`write_handout`) | DOCX in `/data/output` with no name; plain language at or below the target grade; says it does not include vaccines. Open it and check it reads well. |
| 6 | "Compare what you list with MyHealthfinder for a 66-year-old man." (online) | `preventive_services_lookup` | MyHealthfinder's topics (live) are listed with a note that it also covers vaccines and non-USPSTF services; offline, the answer says it was not asked and still gives the USPSTF list. Check a few topics against odphp.health.gov/myhealthfinder. |
| 7 | "Give me the home asthma trigger checklist and a blank asthma action plan for Springfield." | `asthma_home_assessment` | Checklist modules (core, dust mites, mold and moisture) with action steps; housing references for moisture and mold, pests, heat, and smoke and CO alarms with 105 CMR 410 sections read from the housing content (compare with `housing_conditions_health_guide`); DPH directory with Revitalize CDC first ("names your town") and Caring Health Center ("located in your town"), then the regional programs. Blank checklist and action plan DOCX in `/data/output`; the plan has green, yellow, and red zones with blank lines and no medicines. Both content files show the "not yet reviewed" caveat. |
| 8 | "A 9-year-old had asthma symptoms on 12 of the last 30 days, woke 4 nights, and uses the rescue inhaler about 3 times a week. What control category is that?" | `asthma_home_assessment` (`control_answers`) | "Not well controlled" (symptom days 9 to 29, nights 3 to 12; rescue use 0.43 a day), with a note that the person is under 12 and the DPH table is for 12 and older, and that lung function and exacerbations are not part of it. Check the table against DPH's "Factors Associated with Asthma Management in Massachusetts" (appendix). |
| 9 | "Can you use the Asthma Control Test?" | `asthma_home_assessment` | The answer says the ACT is licensed (QualityMetric) and not reproduced, and gives the ACT and ACQ cut scores NHLBI's 2020 update prints. |
| 10 | "Fill asthma action plans from asthma_plans.csv." (the synthetic file) | `asthma_home_assessment` (`clients_path`) | The chat shows only counts (3 people), fields found, and the file name; no names, phone numbers, or dates of birth. Open the plan file: names, doctors, phone numbers, medicines, and doses are from the file; peak flow zones are 80 and 50 percent of the best peak flow (320 gives 256 or more, 160 to 255, under 160); a missing value is a blank line, never an invented medicine or dose. |
| 11 | "Fill asthma action plans from line_list.csv." | `asthma_home_assessment` | Either refused for having no plan columns or filled with counts only; no names, dates of birth, phones, or other values from the file appear in the chat. |

## Content review

- `content/chronic/preventive_services.yaml` (status pending): the nurse should check (1) each
  recommendation's age, sex, pregnancy, and risk-factor criteria against the Task Force's own
  page, especially the ones where the statement gives no age range and the tool chose one (marked
  `assumption`: tobacco prevention in youth, BRCA assessment, hepatitis B screening, PrEP,
  syphilis screening, STI counseling, intimate partner violence ("reproductive age" 15 to 49),
  folic acid (15 to 49), and children's BMI interventions); (2) the "confirm" notes; (3) the
  client summaries, which are the project's plain-language drafts (medium confidence); and (4) the
  Massachusetts programs, read from DPH pages on 2026-11-12 that show no last-updated date
  (MBCCP's eligibility, ages, toll-free number, and seven sites; MCRCCP, which is provider and
  health center facing and has no patient eligibility information; the quit line). Low confidence
  or unverified: whether any statement has been updated after the PubMed record (grades and dates
  could not be checked against the Task Force's site), and Massachusetts programs for lung,
  prostate, hepatitis, or HIV screening (none found on the pages read, so none listed).
- Snapshot `uspstf-ab-recommendations` (built by script, not reviewed content): grade, population,
  and recommendation sentence are copied from the abstracts and the builder asserts the grade tag
  in the text; dates are journal publication dates (the Task Force's posting date can differ by
  days). Not included: statements before 2015 (2008 newborn screening statements) and anything
  not returned by the PubMed search for the Task Force as corporate author; grade C, D, and I
  statements are not listed. Re-run `python3 -I xtask/scripts/build_prevent_snapshots.py --check`
  when the Task Force publishes (the PMIDs are in the script).
- `content/chronic/asthma_home.yaml` (status pending): (1) the checklist questions and actions are
  shortened from the CDC/EPA/HUD June 2018 checklist and EPA's "Asthma Triggers: Gain Control" page
  (high confidence for what they say; check wording before quoting); (2) the control table is the
  one DPH prints in its 2022 asthma management report, a survey adaptation of NHLBI EPR-3 (30-day
  recall, age 12 and older cut points applied to all ages), so it is screening level (medium
  confidence). EPR-3 itself (NCBI Bookshelf, nhlbi.nih.gov) and GINA could not be reached, so the
  clinical grids are NOT bundled; at the table's boundaries (0.29 and 1.00 rescue uses a day) the
  tool uses the more severe category, which is a project decision; (3) the action plan follows
  NHLBI's form (NIH Publication No. 20-HL-5251) as EPA hosts it; the prescriber must complete and
  check any plan; (4) the program directory is DPH's (last updated by DPH in March 2025) and lists
  regions, not towns; DPH's entry for the Boston Public Health Commission links to a Boston
  Children's Hospital page, which the content notes; only Revitalize CDC names towns. Not verified:
  MassHealth coverage of asthma home visits or community health worker services, local board of
  health or housing programs, program phone numbers, and the Massachusetts Health Promotion
  Clearinghouse's asthma action plan forms (host blocked). The ACT link
  (asthmacontroltest.com) was not opened from the build network.

## Open items

- **No USPSTF API adapter and no `PHN_USPSTF_API_KEY`.** www.uspreventiveservicestaskforce.org
  and data.uspreventiveservicestaskforce.org returned the proxy's 403 to CONNECT, so the API's
  key requirement and response format could not be checked, and code for an unverified response
  would have been a guess. The snapshot was built from the published statements instead. If the
  hosts are reachable from the nurse's network, an adapter that refreshes the snapshot is the
  next step (the builder's PMID list would then be replaced by the API's list). Decision for the
  nurse: whether to unblock the host for the builder.
- **The A and B list is not complete.** Statements before 2015 (newborn hearing, PKU, congenital
  hypothyroidism, and sickle cell screening, 2008) were found in PubMed but are not included
  because their current USPSTF status could not be checked; other statements the search did not
  return are also missing. The tool and the snapshot say so. The USPSTF has also had few new
  statements since mid-2025 (the newest in the snapshot is July 2025); check the Task Force's page.
- **Criteria are a project reading of the published population wording.** Only age, sex at birth,
  pregnancy, and 26 risk-factor flags are applied; "average risk", prior disease, and similar conditions are
  left to the nurse ("confirm"). Sex at birth is used for sex-specific recommendations (the
  statements are written by sex at birth or organs present); there are no inputs for hysterectomy
  or organ inventory. Postpartum is a status but the tool does not know the weeks postpartum or
  gestational age (the 24-week gestational diabetes rule is a note). The postmenopausal and fall
  risk inputs are the nurse's judgment.
- **MyHealthfinder has no offline copy** (a single query returns 250 to 460 KB) and it only
  sees age, sex, and pregnancy; the live answer is a cross-check, not a source of grades. The
  response cache holds it for a week.
- **Asthma control is screening level.** No EPR-3 clinical grid (weekly measures, age-specific
  columns for 0 to 4 and 5 to 11, lung function, exacerbation risk) and no GINA; the nurse could
  supply the EPR-3 table from nhlbi.nih.gov and the next pass can add an age-specific classifier.
  The Asthma Control Test is licensed; if the agency holds a license it can be used alongside.
- **Asthma services directory is regional.** Mapping towns to DPH's region names (Western,
  Central, Northeastern, Metro West, Southeast, Metro Boston) needs DPH's town list for those
  regions (not found); until then the tool marks only programs that name or are located in the
  town. The directory has no phone numbers and may be out of date (DPH updated it March 2025).
- **Network lessons.** Blocked: www.uspreventiveservicestaskforce.org and
  data.uspreventiveservicestaskforce.org, www.ahrq.gov, www.healthfinder.gov and health.gov,
  www.cdc.gov (some paths 403 or 404), www.nhlbi.nih.gov and catalog.nhlbi.nih.gov,
  www.ncbi.nlm.nih.gov and pmc.ncbi.nlm.nih.gov web pages, europepmc.org, ginasthma.org, and
  massclearinghouse.ehs.state.ma.us. Working: eutils.ncbi.nlm.nih.gov (PubMed and PMC efetch;
  `db=books` returns only ids, not text), odphp.health.gov (MyHealthfinder API v4,
  `/myhealthfinder/api/v4/myhealthfinder.json?lang=en&age=..&sex=female|male&pregnant=yes`, no key),
  www.epa.gov (asthma pages and the hosted NHLBI action plan PDF), and www.mass.gov (the asthma,
  MBCCP, and colorectal pages; plain curl works, and the PDF `.../download` links work). mass.gov
  asthma pages live under `/orgs/asthma-prevention-and-control` and `/info-details/...`.
- **PubMed as a primary-source route.** USPSTF statements in JAMA carry labelled abstracts
  (Population, Recommendation with "(A recommendation)" tags); `"US Preventive Services Task
  Force"[Corporate Author]` finds them, and efetch returns the labels. Older statements have
  "CONCLUSIONS AND RECOMMENDATION(S)" instead of "Population".
- **Technical debt.** The preventive tool is one long function; the 53 recommendation ids are
  duplicated in the snapshot and the content file (a unit test keeps them equal); the user-guide
  `.docx` needs the coordinator's rebuild; `fixtures/synthetic/README.md` does not list the new
  `asthma_plans.csv` fixture (outside this package's files).
