# Acceptance script: Phase 10 (healthy aging and mental health screening)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `aging` and `bh` toolsets enabled (for example `PHN_TOOLSETS=aging,bh`).
Record pass/fail and notes. Scores, rules, and privacy are also checked automatically
(`crates/phn-server/tests/phase10_tools.rs`, the golden screening cases in
`crates/phn-tools/tests/screening_golden.rs`, and the unit tests in `crates/phn-tools/src/aging/`
and `crates/phn-tools/src/bh/`); this script checks that the answers are correct for
Massachusetts practice and useful.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Score a PHQ-9: 2, 2, 1, 2, 1, 1, 1, 0, 1." | `score_mental_health_screen` | Total 11, moderate, positive; a same-day **safety alert** for item 9 with the ASQ/BSSA steps, 988, and "follow your agency protocol"; no diagnosis. |
| 2 | "ASQ: no, no, yes, no, and yes to question 5." | `score_mental_health_screen` | Acute positive (imminent risk): STAT evaluation, keep in sight, remove dangerous objects, alert the clinician; outside a medical setting 911/988/mobile crisis per protocol. |
| 3 | "An adult refused the ASQ questions. A 15-year-old refused too." | `score_mental_health_screen` | Adult: not a positive screen, document the refusal. Youth: non-acute positive with the BSSA steps. |
| 4 | "Score the EPDS using answer positions: first answer on every item." | `score_mental_health_screen` | Total 21 (items 3 and 5–10 reverse-scored), probable depression, item 10 safety alert. License terms shown; no item wording. |
| 5 | "Score the PHQ-9 screens in phq9_screens.csv." | `score_mental_health_screen` | Counts by severity only (small counts `*`), the number of screens with a safety alert, a results file in `output/` listing alert rows first; no names, IDs, or birth dates. |
| 6 | "Crisis resources for a teenager in Lowell; the family speaks Spanish." | `find_crisis_resources` | 988 (press 2, text Ayuda), BHHL (200+ languages), mobile crisis for youth (877-382-1609), the Vinfen CBHC in Lowell; when to use which. Call-test two numbers. |
| 7 | "Stay Independent: yes to items 1, 3, 4, 6, and 10; TUG 14 seconds; 7 chair stands; man aged 82. Make a plan in Word." | `steadi_fall_risk_assessment` | Score 6, at risk; TUG at risk; chair stand below average (under 10 for men 80–84); interventions by factor; DOCX in `output/`. |
| 8 | "I'm a public health nurse. An 84-year-old client says her son takes her checks. What do I do?" | `elder_protective_services_guide` | Adult Protective Services, (800) 922-2275 or online; mandated reporter; verbal now, written within 48 hours (or online); timelines; financial exploitation definition; no names requested. |
| 9 | "Fill report narratives from protective_report_details.csv." | `elder_protective_services_guide` | Two narratives in `output/`; only counts and field names in the answer. Open one and check the sections against c.19A s.15(e). |
| 10 | "What aging services are there for Pittsfield?" | `find_ma_aging_services` | Elder Services of Berkshire County (ASAP and meals), the SHINE office, MassOptions, ombudsman, Alzheimer's helpline; Council on Aging via MassOptions or the map. |
| 11 | "A 70-year-old couple in Barnstable with $3,900 a month and Medicare: what help is there?" | `explain_older_adult_coverage_options` | Medicare 2026 costs; SLMB/QI (at or below 225% FPL for 2); no MSP asset limit; PACE not available in Barnstable County; SHINE and MassHealth numbers. |
| 12 | "Infection control citations for nursing homes in Worcester." | `nursing_home_quality_lookup` | Facilities with star ratings, staffing, F-tags (F880...) with scope/severity and dates, vaccination rates, MA and U.S. averages, data date and mode (live or snapshot). |
| 13 | "Which tracts in Springfield have the most older adults living alone? Make a map." | `map_older_adult_vulnerability` | Tracts ranked with margins of error and reliability flags, MA and U.S. comparison, map files in `output/`. |
| 14 | "Plan a 45-minute health care proxy session at the senior center and make the handouts. Then fill proxies from proxy_details.csv." | `advance_care_planning_toolkit` | Script totals 45 minutes; c.201D rules (two witnesses, agent not a witness); MOLST flagged unverified with the molst-ma.org warning; three materials and three proxies in `output/`, counted by language (Spanish and Portuguese fall back to English unless `templates/<language>/health_care_proxy.md` exists). |

## Content review (the subject matter expert)

All Phase 10 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

1. `content/bh/mental_health_screening.yaml`: every cutoff and band; the safety-alert rules (PHQ-9
   item 9 and EPDS item 10 at any score above zero; ASQ; C-SSRS risk levels, which come from a
   secondary source because Columbia's site was unreachable); every alert step; and **each
   instrument's license terms** (PHQ/GAD statement not confirmed with the publisher; EPDS terms
   low confidence; C-SSRS terms unverified). Decide which EPDS cut-point your program uses (9/10
   or 12/13).
2. `content/bh/crisis_resources.yaml`: call-test every number; add a verified local warmline if
   your agency uses one.
3. `content/aging/steadi_falls.yaml` against the CDC STEADI materials (chair-stand table,
   thresholds, interventions, Check for Safety).
4. `content/aging/protective_services.yaml` against M.G.L. c.19A and c.19C, 651 CMR 5.00 (not
   retrieved), and DPPC's current site (dppcmass.gov, not reachable); confirm the DPPC hotline.
5. `content/aging/aging_services.yaml` and the directory snapshots (`ma_asap_directory.csv`,
   `ma_shine_directory.csv`, `ma_nutrition_directory.csv`, `ma_cbhc_directory.csv`): spot-check
   five towns, including Boston, a Cape town, and one of the 63 towns with no CBHC "Serving" list.
6. `content/aging/coverage_options.yaml`: update the Medicare figures when CMS publishes 2027
   amounts (November) and the FPL figures each March.
7. `content/aging/nursing_home_guide.yaml`: the scope/severity key (medium confidence).
8. `content/aging/advance_care_planning.yaml`: the proxy template against your agency's form, the
   facilitator script, and **MOLST** (not verified; get the current form from DPH or your health
   system; the old molst-ma.org domain now hosts unrelated content).
