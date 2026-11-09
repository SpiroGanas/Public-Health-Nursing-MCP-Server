# Acceptance script: Phase 12 (emergency preparedness I: situational awareness and planning)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `epr`, `env`, and `core` toolsets enabled (for example
`PHN_PROFILE=preparedness` or `PHN_TOOLSETS=epr,env`). Run prompts 1, 3, 4, and 7 once with the
internet and once offline (`PHN_OFFLINE=1`). Record pass/fail and notes. The calculations are
also checked automatically (`crates/phn-epi/tests/queueing_golden.rs`,
`crates/phn-server/tests/phase12_tools.rs`, and unit tests in `crates/phn-tools/src/epr/` and
`env/`); this script checks that the answers are right for Massachusetts practice and useful.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Are there any weather alerts for Bristol County right now, and what should the health department do?" | `get_active_hazard_alerts` | Live: current NWS alerts (or "no active alerts") with the feed's update time; each hazard's health effects, at-risk groups, actions, and messages are cited. Offline: the first caveat says alerts could NOT be retrieved and no alerts are invented. FEMA declarations listed (DR-4780-MA, September 2023 flooding, should appear for Bristol). |
| 2 | "We need to give antibiotics to 48,000 people with 2 PODs open 48 hours. How many staff at each station, and where is the bottleneck?" | `model_dispensing_site_throughput` | 500 arrivals per hour per site; staff by station with utilization ≤ 85% and waits ≤ 10 minutes; bottleneck and capacity; hours and days; station times labeled tool default. Then give your own drill times and check the numbers change. |
| 3 | "What is the air quality in Pittsfield today and tomorrow, and can the elementary school hold recess outside?" | `air_quality_protective_guidance` | Nearest AirNow reporting area (Berkshire) with distance; current and forecast AQI; school action quoted from EPA's school guidance. Offline: says AirNow could not be reached and lists actions for every category. |
| 4 | "Make a heat response plan for Lowell for this week, using cooling_centers.csv." | `heat_health_action_planner` | 7-day HeatRisk by day and forecast highs (online); local population table (65+, disability, PLACES, SVI, emPOWER); activation steps for the peak level; checklist DOCX in `output/` with the four cooling centers. The contact person names, cell phones, and emails in the file must NOT appear in the chat. Open the checklist and confirm the outreach scripts and heat illness table read well. |
| 5 | "Start a hazard vulnerability assessment for Plymouth County. Score hurricanes 2, 3, 3, 2 with gaps 2, 2, 1 and power outage 3, 2, 1, 2 with gaps 2, 2, 2." | `hazard_vulnerability_assessment` | Power outage ranks first (61.1%), hurricanes second (48.1%); ResilientMass seeds and FEMA declaration counts for all 21 hazards; XLSX workbook in `output/`; a caveat that the National Risk Index could not be bundled. |
| 6 | "We expect 200 people at the Brockton high school shelter. What functional needs should we plan for, and how many staff? Then fill intake forms from shelter_registrations.csv." | `functional_needs_shelter_intake` | CMIST questions; estimates by need with local rates; staff per shift labeled tool default; completed forms in `output/`; counts by need and language only, small counts shown as `<5`; no names, birth dates, phones, addresses, or emails in the chat. |
| 7 | "How many people in Plymouth County rely on electricity-dependent medical equipment? List the ZIP codes." | `get_electricity_dependent_population` | County total and rate per 1,000 compared with MA and the U.S.; ZIP list with values of 11 marked 1–11; live online, snapshot offline (says so); utility numbers and actions. |
| 8 | "Plan a CASPER for Fall River with seed 2026." Then: "Analyze casper_results.csv for structure, damage, has_generator, and needs_medication; 400 households were attempted and 260 contacted." | `plan_casper_assessment` | 30 clusters (census block GEOIDs, households, interviews, sampling interval); the same seed gives the same clusters; materials in `output/`. Analysis: 198 interviews, completion 94.3%, contact and cooperation rates, weighted percentages with 95% CIs and estimated households, cells under 5 hidden; no addresses or phone numbers from the file in the chat. |

## Content review (the subject matter expert)

All Phase 12 content is `status: pending` (review due 2027-02-01). Check each file against its
cited sources, then set `meta.review` (see `CONTRIBUTING.md`):

1. `content/epr/hazard_health_actions.yaml`: hazard-to-NWS-event mapping (old and new NWS heat
   and cold product names), and each public health action (medium-confidence items are this
   project's suggestions).
2. `content/epr/dispensing.yaml`: replace the tool-default station times and shares with your
   POD exercise data; confirm with DPH whether a dispensing time target (for example the Cities
   Readiness Initiative's) should be built in.
3. `content/env/air_quality.yaml`: breakpoints against 40 CFR 58 Appendix G; the general-public
   and outdoor-event actions (summaries) against AirNow's Air Quality Guides when available.
4. `content/env/heat.yaml`: activation steps by HeatRisk level and the three outreach scripts
   (drafts) against your heat plan and DPH guidance.
5. `content/epr/hva.yaml`: the scoring method (the tool's own), the seed mappings, and the six
   local hazards; whether to align with your HMCC's HVA tool.
6. `content/epr/functional_needs_shelter.yaml`: the intake questions (draft) and the staffing
   ratios (tool defaults) against your shelter plan and MEMA/Red Cross forms.
7. `content/epr/electricity_dependence.yaml`: utility phone numbers and the dataset access text.
8. `content/epr/casper.yaml`: the consent script and tracking-form outcomes (drafts).
