# Acceptance script: Phase 9 (PHN practice toolkit and case management)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `cd`, `equity`, and `core` toolsets enabled (for example
`PHN_PROFILE=cd_nurse`, or `PHN_TOOLSETS=cd,equity`). Set your town first ("Configure my town as
Amherst"). Record pass/fail and notes. The rules are also checked automatically
(`crates/phn-server/tests/phase9_tools.rs` and the unit tests in `crates/phn-tools/src/cd/`,
`core/`, and `equity/`); this script checks that the answers are right for Massachusetts practice
and useful.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Plan a TB contact investigation: smear-positive cavitary pulmonary TB, cough since June 10, 2026, first abnormal chest X-ray August 1, treatment started August 5. Contacts: 4 adults in the household (last exposure August 5), 6 children under 5 at a child care center, 20 coworkers. Contacts listed August 7." | `plan_tb_contact_investigation` | Infectious period from March 10 (3 months before onset); household and children high priority, coworkers low; first encounters due August 18 (7 business days); retest window September 30 to October 14; window prophylaxis for the children; DPH TB phone numbers; workbook in `output/`. Open the workbook and change one contact's Household to N: the priority and due dates update. |
| 2 | "Use tb_contacts.csv for this investigation. Should we expand?" | `plan_tb_contact_investigation` | Counts by priority only, no names or phone numbers; expansion signals include infection in a child under 5 and a negative-to-positive change. |
| 3 | "Make a 3HP calendar starting January 5, 2026. She took doses January 5, 12, and 19 and none since; today is March 30." | `ltbi_treatment_calendar` | 12 weekly doses; complete 11 by April 26; status: cannot complete in the window; gap over 2 months (rule out TB disease before resuming); says it does not calculate doses; tracker and calendar in `output/`. |
| 4 | "Same calendar for 4R in Portuguese." | `ltbi_treatment_calendar` | 120 daily doses within 6 months; English calendar with a caveat that no agency translation exists in `templates/portuguese/`. Add a translation file and repeat: it is used. |
| 5 | "Score this AHC screen: Q1 'I do not have a steady place to live', Q3 often true, Q4 never true, Q5 no, Q6 already shut off, safety 2, 3, 3, 3." | `screen_health_related_social_needs` | Living situation, food, utilities, and safety positive (HITS 11); safety caveat first; Massachusetts referrals (EA shelter, SNAP/WIC, HEAP, Mass 211). |
| 6 | "Summarize hrsn_screenings.csv by site." | `screen_health_related_social_needs` | Counts by domain and site with small cells as `*`; no names, MRNs, or birth dates; per-person file in `output/`. |
| 7 | "Code client_note.txt with the Omaha System and draft a care plan." | `omaha_system_coder` | Problems with codes (for example 42 Medication regimen); no text from the note in the answer; care plan DOCX in `output/`. |
| 8 | "Classify these activities by the Intervention Wheel: drafted a tobacco regulation; flu shots at the senior center under standing orders; measles contact tracing; taught a car seat class." | `classify_intervention_wheel` | Policy development and enforcement (systems), delegated functions, disease and health event investigation, health teaching; definitions with confidence levels. |
| 9 | "Give me a DAR template for a DOT visit, then check client_note.txt as a home visit note." | `nursing_documentation_template` | Template with required elements (dose observed, side-effect screen); the check lists missing elements (signature, follow-up...) without quoting the note. |
| 10 | "Find a food pantry near Amherst with Spanish-speaking staff open Saturday at 10." | `find_community_resources` | The fictional pantry from `resources/community_resources.csv`; stale entries flagged when asked for transportation. "Which health centers in Springfield offer dental?" lists HRSA sites (live, or snapshot offline). |
| 11 | "An asylee, 45, arrived September 1, 2026. What screening is due, and what MassHealth coverage applies now? Make a checklist; she speaks Dari." | `newcomer_health_navigator` | Screening by November 30; IGRA (age 2+), hepatitis B and C, HIV; Group 2 under EOM 26-10: MassHealth Limited/Health Safety Net from October 1, 2026; RHAP and RHPP contacts; interpreter reminder; English checklist with a translation caveat. |

## Content review (the subject matter expert)

All Phase 9 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

1. `content/cd/tuberculosis.yaml`: the priority trees against Figures 2-4 and Table 3 of the 2005
   guidelines; the completion windows (3HP's 11 doses in 16 weeks comes from the trial CDC cited
   in 2011; no window is published for 3HR, so agree one with DPH); the DPH TB clinic list (2025).
   Decide whether the tool should default to DPH's three regimens only.
2. `content/equity/ahc_hrsn.yaml`: the scoring against the CMS form (the underlined answers were
   read from the PDF); the reuse status of each item (CMS's permission covers the AHC Model only;
   HITS and PRAPARE have their own terms) — decide whether your agency may use the full wording;
   the referral resources and phone numbers.
3. `content/core/omaha_system.yaml`: problem and target codes and the signs/symptoms marked
   unverified, against Appendix A of the 2005 Omaha System book; rating labels 4 and 5.
4. `content/core/intervention_wheel.yaml`: definitions marked low confidence (surveillance,
   screening, delegated functions, consultation, and the three levels) and the wedge colors,
   against MDH's *Public Health Interventions* (2nd ed., 2019).
5. `content/core/documentation_templates.yaml`: replace the tool-default structures and prompts
   with your agency's documentation policy; confirm the required elements.
6. `content/equity/newcomer_health.yaml`: the status-to-group table against MassHealth's
   noncitizen rules and EOM 26-10; the Refugee Medical Assistance length (ORI's pages say 8 and 4
   months); the screening summaries against the CDC pages.
