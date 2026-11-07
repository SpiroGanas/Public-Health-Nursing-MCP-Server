# Public Health Nursing MCP Server: Tool Brainstorm

This document brainstorms Model Context Protocol (MCP) tools that would make an AI assistant more useful to a public health nurse working for a Massachusetts city or town, a school district, or the Massachusetts Department of Public Health (DPH).

It lists **172 tools in 17 themed sections**. 148 of them can be built from public data, published guidance, and the nurse's own files. The other 24 need an outside resource, such as a purchased license, a state data-access agreement, or professional sign-off. They are marked **Outside** and are collected in Appendix A of [`project_plan.md`](project_plan.md).

---

## Who the tools are for

The user is a registered nurse with a Doctor of Nursing Practice (DNP) degree in the Public Health Nurse Leader (PHNL) concentration. This profile is modeled on the UMass Amherst Elaine Marieb College of Nursing DNP-PHNL program. The program prepares nurses for leadership in population-level health assessment and intervention, and it frames practice with:

- the **AACN *Essentials* (2021)**: ten domains, with Domain III Population Health at the center of this work;
- the **ANA *Public Health Nursing: Scope and Standards of Practice* (2022)**;
- **750 supervised PHNL practicum hours** (three 250-hour practicums), plus DNP project hours spent translating evidence into a program at a public health site.

| Area of training | Coursework | What it means for tool design |
|---|---|---|
| Epidemiology and biostatistics | N540 Epidemiology for Clinicians; N715 Intermediate Statistics; N630 Research Methodology | Show methods, confidence intervals, and assumptions. Do not hide the math or oversimplify. |
| Community and population assessment | N640 Advanced PHN I with N698G practicum (community and targeted needs assessment) | One-call community profiles, prioritization tools, and needs assessment workflows. |
| Program planning, evaluation, and grants | N750 Advanced PHN II with N798LL practicum (program design, monitoring, evaluation, grant writing) | Logic models, evaluation plans, funding-announcement analysis, and budgets. |
| Implementation and contemporary issues | N760 Contemporary Issues with N798M practicum (political, cultural, ethical, environmental factors) | Implementation tracking and policy, ethics, and climate tools. |
| Health equity and social justice | HPP605 Health Equity and the Foundations of Public Health; N704 Health Disparities and Social Justice | Break results down by race/ethnicity, language, and income by default where the data allow. |
| Environmental health | EHS565 Environmental Health Practices (water, wastewater, solid waste, food sanitation, vector control, housing, accident control) | Water, housing, vector, and climate tools. |
| Behavior change | HPP601 Social and Behavioral Theories in Health Education and Intervention | Theory-based intervention design and messaging. |
| Finance and leadership | HPP612 Financial Management; N725 Leadership in Health Systems | Budget, staffing, strategic planning, and accreditation tools. |
| Policy, ethics, quality, informatics | N651 Nursing Ethics, Health Policy and Politics; N701 Healthcare Quality; N735 Informatics | Legislative tracking, ethics frameworks, QI/SPC charts, and data-handling tools. |
| Evidence translation | N742, N798U, N840, N898A (DNP project sequence) | Literature search, appraisal, synthesis, and QI-versus-research determination. |

**Design implications**

1. **Expert user.** Tools should expose methods and sources rather than explain basics. The nurse is the expert in the loop. Tools support decisions and never make them.
2. **Population focus.** Most tools work at the community, school, or program level, not only on individual care.
3. **Evidence and citations.** Every answer names its source, publisher, and effective or retrieval date.
4. **Professional deliverables.** Many tools produce documents that can be used as-is: reports, plans, letters, grant sections, and board materials.
5. **Equity lens.** Tools stratify by available equity dimensions and flag small numbers.

## Design assumptions

- **Massachusetts-specific.** Tools are built around DPH, Massachusetts regulations (105 CMR, 244 CMR), the Massachusetts General Laws, the 351 cities and towns and their boards of health, and Massachusetts programs. Federal data is used for comparison.
- **Data scope.** Tools use public and aggregate data, plus files the nurse places in a folder mounted into the container, such as a de-identified line list. There are no direct connections to systems containing protected health information (PHI), such as the Massachusetts Immunization Information System (MIIS), MAVEN, or electronic health records. Those tools are marked **Outside**.
- **Everything a tool returns is sent to the AI model.** Tools that read local files therefore return aggregates by default. Anything that must contain names or addresses, such as letters or contact lists, is written directly to the output folder and never returned to the model.
- **Deployment.** Each nurse runs the Docker container on their own computer. An MCP client such as Claude Desktop launches it over stdio.
- **Network.** Tools call public APIs live and fall back to data snapshots bundled in the image when offline or blocked.

## Legend

| Tag | Meaning |
|---|---|
| **Live** | Calls a public API or website. A bundled snapshot is used when offline. |
| **Snapshot** | Uses reference content or data bundled in the image and refreshed with each release. |
| **Local** | Reads or writes files in the nurse's mounted folder. |
| **Calc** | Does a calculation inside the server. |
| **Doc** | Generates a document, template, or form (DOCX, XLSX, PDF, or SVG). |
| **Outside** | Cannot be built without an outside resource. See `project_plan.md`, Appendix A. |

## Summary

| # | Section | Tools | Outside | Builds on |
|---|---|---|---|---|
| 1 | [Immunizations and Vaccine-Preventable Disease](#1-immunizations-and-vaccine-preventable-disease-vax) | 11 | 2 | N540, N640; Domain III |
| 2 | [Communicable Disease Surveillance and Control](#2-communicable-disease-surveillance-and-control-cd) | 18 | 5 | N540, EHS565, N640 |
| 3 | [School Health](#3-school-health-sch) | 9 | 1 | HPP601, N640 |
| 4 | [Maternal, Child, and Family Health](#4-maternal-child-and-family-health-mch) | 10 | 3 | N640, N704 |
| 5 | [Healthy Aging and Older Adults](#5-healthy-aging-and-older-adults-age) | 9 | 2 | N640, N704 |
| 6 | [Mental Health and Substance Use](#6-mental-health-and-substance-use-mh) | 7 | 0 | N640, HPP601 |
| 7 | [Public Health Emergencies and Disaster Preparedness](#7-public-health-emergencies-and-disaster-preparedness-epr) | 13 | 2 | N725, N760 |
| 8 | [Environmental Health and Climate](#8-environmental-health-and-climate-env) | 8 | 0 | EHS565, N760 |
| 9 | [Chronic Disease Prevention and Health Promotion](#9-chronic-disease-prevention-and-health-promotion-chr) | 7 | 0 | HPP601, N750 |
| 10 | [Health Equity and Community Assessment](#10-health-equity-and-community-assessment-eq) | 12 | 1 | HPP605, N704, N640 |
| 11 | [Epidemiology, Biostatistics, and Data](#11-epidemiology-biostatistics-and-data-epi) | 15 | 2 | N540, N715, N735 |
| 12 | [Program Planning, Evaluation, and Grants](#12-program-planning-evaluation-and-grants-plan) | 11 | 0 | N750, HPP601 |
| 13 | [Leadership, Finance, and Administration](#13-leadership-finance-and-administration-lead) | 9 | 1 | N725, HPP612, N701 |
| 14 | [Policy, Law, and Ethics](#14-policy-law-and-ethics-pol) | 8 | 1 | N651, N760 |
| 15 | [Health Communication and Language Access](#15-health-communication-and-language-access-comm) | 6 | 1 | HPP601, N704 |
| 16 | [Evidence-Based Practice and Quality Improvement](#16-evidence-based-practice-and-quality-improvement-ebp) | 8 | 1 | N630, N742, N701 |
| 17 | [PHN Practice Toolkit and Platform Utilities](#17-phn-practice-toolkit-and-platform-utilities-core) | 11 | 2 | N735; ANA Standards |
| | **Total** | **172** | **24** | |

---

## 1. Immunizations and Vaccine-Preventable Disease (VAX)

These tools cover running clinics, keeping schools compliant, catching children up on missed doses, and answering families' questions. Federal and professional-society recommendations have diverged since 2025, so the tools always show which body a recommendation comes from and its date.

| ID | Tool | Description | Source |
|---|---|---|---|
| VAX-01 | `get_immunization_schedule` | Returns the current child/adolescent and adult schedules with minimum ages, minimum intervals, contraindications, and precautions, labeled with edition dates. | Snapshot (CDC, DPH) |
| VAX-02 | `compare_vaccine_recommendations` | Shows CDC/ACIP, DPH, and professional-society (AAP, ACOG, AAFP) recommendations side by side for a vaccine and population and highlights where they differ. Useful for standing orders and parent questions. | Snapshot + Live |
| VAX-03 | `forecast_immunizations_due` | Evaluates a dose history (typed in or from a local file) and returns valid and invalid doses, next due dates, and a catch-up schedule. It implements the CDC Clinical Decision Support for Immunization (CDSi) logic and is tested against CDC's published CDSi test cases. | Calc + Snapshot |
| VAX-04 | `check_ma_school_immunization_requirements` | Checks a record against Massachusetts requirements for child care, preschool, K–12, and college (105 CMR 220.000 and DPH's annual requirements document), including acceptable proof of immunity and exemption rules. | Snapshot + Calc |
| VAX-05 | `analyze_vaccination_coverage` | Combines DPH's school-level kindergarten and 7th-grade immunization and exemption data with National Immunization Survey and adult coverage estimates. Ranks schools and towns and flags those below herd-immunity thresholds (for example, 95% for measles). | Snapshot + Live |
| VAX-06 | `vaccine_temperature_excursion_helper` | Walks through a cold-chain excursion step by step: isolate and label stock, record minimum/maximum temperatures and duration, prepare for the manufacturer call, and document it under the CDC Storage and Handling Toolkit and DPH Vaccine Unit rules. | Snapshot + Doc |
| VAX-07 | `lookup_vaccine_product` | Product reference by name, NDC, or CVX code: manufacturer, presentations, licensed ages, storage, beyond-use dates, CVX/MVX codes, and current Vaccine Information Statement (VIS) editions by language. | Live + Snapshot |
| VAX-08 | `plan_vaccination_clinic` | Plans a community or school-located clinic from expected turnout. Covers stations, throughput, staffing, supplies, an anaphylaxis kit checklist, layout, and a post-clinic dose reconciliation sheet. | Calc + Doc |
| VAX-09 | `vaccine_confidence_conversation_guide` | Builds motivational-interviewing responses to a specific concern, with citations to CDC, AAP, and Immunize.org, at a chosen reading level and language. | Snapshot + Doc |
| VAX-10 | `query_miis_immunization_record` | Looks up a client's immunization history and forecast in MIIS. | **Outside** |
| VAX-11 | `send_immunization_reminders` | Sends reminder and recall messages (text, email, voice) to clients who are due or overdue. | **Outside** |

## 2. Communicable Disease Surveillance and Control (CD)

Massachusetts boards of health must investigate and control reportable diseases under 105 CMR 300.000. These tools support that daily work, from the first lab report to the end of an outbreak.

| ID | Tool | Description | Source |
|---|---|---|---|
| CD-01 | `lookup_reportable_condition` | For a disease or lab result, says whether it is reportable in Massachusetts, who reports it (provider, lab, local board of health), how fast (immediately or within 24 hours), and to whom. Links the matching chapter of DPH's guide. | Snapshot |
| CD-02 | `search_ma_disease_control_guide` | Offline full-text search of DPH's *Guide to Surveillance, Reporting and Control* and DPH disease fact sheets. Returns quoted passages with chapter citations. | Snapshot |
| CD-03 | `calculate_isolation_and_exclusion` | Computes isolation, quarantine, and exclusion dates from the disease, onset/test/exposure dates, and setting (child care, school, food handler, health care), under 105 CMR 300.000 and DPH guidance. | Calc + Snapshot |
| CD-04 | `analyze_outbreak_line_list` | Loads a local line list (CSV/XLSX), applies the case definition, and returns person/place/time summaries, attack rates, 2x2 tables of exposures with relative risks, odds ratios, and confidence intervals, and the probable exposure window based on incubation periods. Returns aggregates only. | Local + Calc |
| CD-05 | `generate_epi_curve` | Draws an epidemic curve (SVG/PNG) with a time interval of about one-quarter to one-third of the incubation period, optional stratification, and incubation-period markers. Describes the likely pattern (point source, continuous, or propagated). | Calc |
| CD-06 | `plan_contact_tracing` | From an index case's dates, computes the infectious period, contact elicitation window, testing dates, and monitoring end dates. Writes a contact tracking spreadsheet to the output folder. | Calc + Doc |
| CD-07 | `assess_measles_exposure` | For a measles exposure: checks evidence of immunity, the post-exposure prophylaxis windows (MMR within 72 hours, immune globulin within 6 days), exclusion of susceptible contacts from day 5 through day 21, and priority groups. | Calc + Snapshot |
| CD-08 | `get_notifiable_disease_trends` | Pulls weekly and annual National Notifiable Diseases Surveillance System (NNDSS) counts for Massachusetts, New England, and the US, and compares the current period with a 5-year baseline. | Live |
| CD-09 | `get_respiratory_and_wastewater_surveillance` | Summarizes COVID-19, influenza, and RSV activity (CDC respiratory data, DPH dashboard) and wastewater trends by sewershed (CDC NWSS, MWRA) for weekly briefings. | Live + Snapshot |
| CD-10 | `detect_aberrations` | Runs CDC EARS (C1–C3), CUSUM, and Farrington-type algorithms on a case or syndrome time series. Explains any signal and the sensitivity trade-offs. | Calc |
| CD-11 | `plan_tb_contact_investigation` | Applies CDC/NTCA contact investigation guidance: infectious period, contact priority (high, medium, low), and testing schedule, including the retest 8–10 weeks after last exposure. Produces a tracking sheet. | Calc + Snapshot |
| CD-12 | `ltbi_treatment_calendar` | Builds a calendar and adherence tracker for a chosen latent TB infection (LTBI) regimen, such as 3HP, 4R, or 3HR: dose count, monitoring visits, and completion window. Does not calculate doses. | Calc + Doc |
| CD-13 | `infection_control_for_congregate_settings` | Covers transmission-based precautions, cohorting, PPE, and disinfectant selection (EPA-registered products by pathogen) for shelters, schools, long-term care, and day programs. | Snapshot |
| CD-14 | `sync_maven_case` | Reads and updates case investigations in MAVEN, the state surveillance system. | **Outside** |
| CD-15 | `query_syndromic_surveillance` | Queries emergency department syndromic data (NSSP ESSENCE) for the jurisdiction. | **Outside** |
| CD-16 | `calculate_tb_medication_doses` | Calculates weight-based doses for TB and LTBI regimens. | **Outside** |
| CD-17 | `draft_isolation_quarantine_order` | Drafts board of health isolation or quarantine orders and notices. | **Outside** |
| CD-18 | `search_licensed_id_references` | Searches licensed infectious disease references (APHA *Control of Communicable Diseases Manual*, AAP *Red Book*). | **Outside** |

## 3. School Health (SCH)

School nurses in Massachusetts work under several DPH and EEC regulations and produce yearly data for DPH. These tools cover screening, medications, health plans, and reporting.

| ID | Tool | Description | Source |
|---|---|---|---|
| SCH-01 | `get_ma_school_screening_requirements` | Lists required school health screenings under Massachusetts law and 105 CMR 200.000 (vision, hearing, postural, and others) by grade, with protocols, referral criteria, and parent-notification rules. | Snapshot |
| SCH-02 | `track_school_screenings` | From a local roster and results file, lists who is due, computes completion and referral follow-up rates by grade, and mail-merges referral letters in each family's language into the output folder. Student names are never sent to the AI model. | Local + Doc |
| SCH-03 | `school_medication_administration_guide` | Answers questions about 105 CMR 210.000 with citations: medication orders and consent, delegation to unlicensed staff, field trips, stock epinephrine and naloxone, and DPH registration. | Snapshot |
| SCH-04 | `build_individualized_health_plan` | Drafts individualized health plans and emergency action plans for diabetes, asthma, seizures, and food allergy. Includes nursing goals and interventions and suggests Section 504 accommodations. | Snapshot + Doc |
| SCH-05 | `concussion_return_to_learn_tracker` | Applies 105 CMR 201.000 (forms, training, removal from play, medical clearance) and builds graduated return-to-learn and return-to-play plans with a tracking sheet. | Snapshot + Doc |
| SCH-06 | `monitor_school_absenteeism` | Uses daily absence counts with illness reasons to spot influenza-like or gastrointestinal spikes by school. Flags when to notify the local board of health. | Local + Calc |
| SCH-07 | `build_school_health_annual_report` | Compiles year-end statistics from local health office logs (visits, return-to-class rate, dismissals, screenings, medication doses, emergencies) into DPH's annual school health data format and a summary for the school committee. | Local + Doc |
| SCH-08 | `child_care_health_consultation_checklist` | Checklist for child care health consultants against EEC licensing rules (606 CMR 7.00): health care policy, medication administration, immunization records, and illness exclusion. | Snapshot |
| SCH-09 | `sync_student_health_records` | Reads and writes student health records in district systems (for example SNAP Health Center, Aspen, or PowerSchool). | **Outside** |

## 4. Maternal, Child, and Family Health (MCH)

These tools support home visiting, early childhood screening, lead poisoning prevention, and child protection.

| ID | Tool | Description | Source |
|---|---|---|---|
| MCH-01 | `plan_newborn_postpartum_visit` | Builds a home visit plan and teaching sheet for newborn and postpartum visits: feeding and weight, jaundice, safe sleep (AAP 2022), car seats, newborn screening follow-up, and urgent maternal warning signs (CDC *Hear Her*). | Snapshot + Doc |
| MCH-02 | `check_developmental_milestones` | Lists CDC "Learn the Signs. Act Early." milestones (2022 revision) for a child's age, flags missed milestones, and gives Massachusetts Early Intervention referral steps. | Snapshot |
| MCH-03 | `calculate_growth_percentiles` | Calculates percentiles and z-scores from WHO (0–2 years) and CDC (2–20 years) growth charts, including CDC's extended BMI-for-age charts and corrected age for prematurity. | Calc + Snapshot |
| MCH-04 | `childhood_lead_screening_guide` | Covers the Massachusetts lead screening schedule (105 CMR 460.000), DPH's high-risk community list, venous confirmation timing by blood lead level, case management steps, and family teaching. | Snapshot |
| MCH-05 | `map_lead_risk` | Ranks towns and census tracts for lead outreach using pre-1950 and pre-1978 housing (American Community Survey), poverty, DPH high-risk designations, and published screening rates. | Live + Snapshot |
| MCH-06 | `maternal_infant_health_profile` | Reports births, preterm birth, low birth weight, prenatal care, infant mortality, and Massachusetts PRAMS indicators by community and race/ethnicity, compared with state and national figures. | Live + Snapshot |
| MCH-07 | `mandated_reporter_51a_guide` | Explains Massachusetts child abuse and neglect reporting (M.G.L. c.119 §51A): who must report, an oral report immediately and a written report to DCF within 48 hours, and what to include. Drafts the written narrative from the nurse's notes. | Snapshot + Doc |
| MCH-08 | `score_asq_developmental_screen` | Scores Ages & Stages Questionnaires (ASQ-3, ASQ:SE-2). | **Outside** |
| MCH-09 | `optimize_home_visit_routes` | Orders a day's home visits into an efficient driving route. | **Outside** |
| MCH-10 | `manage_client_caseload` | Tracks identifiable clients across home visiting, lead, and TB case management: due visits and overdue follow-ups. | **Outside** |

## 5. Healthy Aging and Older Adults (AGE)

These tools connect older adults to the Massachusetts aging network, help prevent falls, and support protective services, coverage decisions, and advance care planning.

| ID | Tool | Description | Source |
|---|---|---|---|
| AGE-01 | `steadi_fall_risk_assessment` | Scores the CDC STEADI *Stay Independent* screener and interprets Timed Up and Go, 30-Second Chair Stand, and 4-Stage Balance results against norms. Builds an intervention and home-safety plan. | Calc + Snapshot |
| AGE-02 | `find_ma_aging_services` | For a town, lists the Aging Services Access Point, Council on Aging, SHINE health insurance counseling, home-delivered meals, caregiver and dementia supports, and protective services contacts. | Snapshot |
| AGE-03 | `elder_protective_services_guide` | Explains Massachusetts reporting rules for abuse, neglect, and self-neglect of adults 60 and older, and for adults with disabilities aged 18–59 (Disabled Persons Protection Commission). Covers mandated reporters, hotlines, and documentation, and drafts the report narrative. | Snapshot + Doc |
| AGE-04 | `explain_older_adult_coverage_options` | Gives a plain-language comparison of Medicare parts, Medicare Savings Programs, MassHealth, the Frail Elder Waiver, PACE, Senior Care Options, and One Care, with current income and asset limits. | Snapshot |
| AGE-05 | `map_older_adult_vulnerability` | Maps older adults who live alone, have disabilities, lack a vehicle, have limited English, or live in poverty (American Community Survey) by tract or town. For outreach, wellness checks, and emergency planning. | Live + Calc |
| AGE-06 | `nursing_home_quality_lookup` | Pulls CMS Care Compare data for Massachusetts nursing homes: star ratings, staffing, inspection citations (including infection control), and resident and staff vaccination rates. | Live |
| AGE-07 | `advance_care_planning_toolkit` | Provides the Massachusetts Health Care Proxy and MOLST forms with instructions, The Conversation Project guides, and a facilitator script for community advance care planning sessions. | Snapshot + Doc |
| AGE-08 | `score_cognitive_screen` | Scores cognitive screening tools such as MoCA, MMSE, and Mini-Cog. | **Outside** |
| AGE-09 | `review_older_adult_medications` | Flags potentially inappropriate medications and interactions using the AGS Beers Criteria. | **Outside** |

## 6. Mental Health and Substance Use (MH)

These tools connect people to crisis and treatment services, support screening and overdose prevention, and help schools respond after a suicide.

| ID | Tool | Description | Source |
|---|---|---|---|
| MH-01 | `find_crisis_resources` | For a town: 988, the Massachusetts Behavioral Health Help Line, the local Community Behavioral Health Center, youth mobile crisis, and warmlines, with phone, text, chat, and language options. | Snapshot |
| MH-02 | `score_mental_health_screen` | Scores free screening tools (PHQ-2/PHQ-9, GAD-7, Edinburgh Postnatal Depression Scale, NIMH ASQ, C-SSRS screener) and interprets cutoffs. Shows required safety steps when a risk item is positive. Does not diagnose. | Calc + Snapshot |
| MH-03 | `get_overdose_data` | Reports DPH opioid-related overdose death data by city and town, plus CDC provisional overdose counts, with trends and comparisons. | Live + Snapshot |
| MH-04 | `find_naloxone_and_harm_reduction` | Lists free naloxone sites, pharmacies under the statewide standing order, syringe service programs, and fentanyl test strip sources by town. Includes a naloxone teaching script. | Snapshot |
| MH-05 | `find_substance_use_treatment` | Searches FindTreatment.gov and the Massachusetts Substance Use Helpline directory for medications for opioid use disorder, withdrawal management, and recovery supports, filtered by town, insurance (including MassHealth), and language. | Live + Snapshot |
| MH-06 | `plan_school_postvention` | After a suicide affecting a school: steps from the AFSP/SPRC *After a Suicide* toolkit, templates for staff, student, and family messages, and a safe-messaging check of drafts. | Snapshot + Doc |
| MH-07 | `get_youth_risk_behavior_data` | Reports Massachusetts Youth Health Survey/YRBS and CDC YRBS indicators (persistent sadness, suicidal ideation, bullying, and others) by sex, grade, and race/ethnicity, compared with the US. | Live + Snapshot |

## 7. Public Health Emergencies and Disaster Preparedness (EPR)

These tools cover the preparedness cycle: assessing hazards, planning, exercises, response operations, and after-action improvement. They are aligned with FEMA ICS/HSEEP, the CDC Public Health Emergency Preparedness (PHEP) capabilities, and Massachusetts Health and Medical Coordinating Coalitions (HMCCs).

| ID | Tool | Description | Source |
|---|---|---|---|
| EPR-01 | `generate_ics_forms` | Fills FEMA ICS forms (201, 202, 203, 204, 205A, 206, 213, 214, 215A) from incident details and assembles them into an Incident Action Plan (DOCX/PDF). | Doc |
| EPR-02 | `model_dispensing_site_throughput` | Models Emergency Dispensing Site (EDS/POD) throughput, staff per station, queue lengths, and the hours needed to reach a population with medical countermeasures. | Calc |
| EPR-03 | `functional_needs_shelter_intake` | Provides a shelter health intake using the CMIST framework (communication, maintaining health, independence, support and safety, transportation). Estimates support needs, supplies, and staffing for a shelter population. | Calc + Doc |
| EPR-04 | `get_electricity_dependent_population` | Reports HHS emPOWER counts of Medicare beneficiaries who rely on electricity-dependent medical equipment or home health, by ZIP code and county. For power outage and evacuation planning. | Live + Snapshot |
| EPR-05 | `get_active_hazard_alerts` | Lists current National Weather Service alerts for Massachusetts zones and FEMA disaster declarations, with likely health effects and suggested actions. | Live |
| EPR-06 | `hazard_vulnerability_assessment` | Facilitates a jurisdictional hazard vulnerability assessment (probability, impact, preparedness), seeded with FEMA National Risk Index data. | Live + Calc |
| EPR-07 | `continuity_of_operations_planner` | Builds a local health department continuity of operations (COOP) plan: essential functions, orders of succession, delegations of authority, alternate sites, and vital records. | Doc |
| EPR-08 | `design_hseep_exercise` | Designs HSEEP-consistent tabletop and functional exercises with objectives mapped to the 15 CDC PHEP capabilities. Includes the scenario, injects, situation manual, and evaluation guides. | Doc |
| EPR-09 | `build_after_action_report` | Turns hotwash notes into an HSEEP after-action report and improvement plan: strengths, areas for improvement, corrective actions, owners, and dates. | Doc |
| EPR-10 | `crisis_risk_communication_builder` | Uses CDC's Crisis and Emergency Risk Communication (CERC) model to build message maps, holding statements, FAQs, and spokesperson prep for each phase of a crisis, adapted for Massachusetts' most common languages. | Snapshot + Doc |
| EPR-11 | `plan_casper_assessment` | Plans a CDC CASPER rapid needs assessment: a two-stage cluster sample of census blocks, questionnaire, team materials, and weighted analysis of the returned data. | Calc + Live + Doc |
| EPR-12 | `publish_maps_to_arcgis` | Publishes maps and layers to the agency's ArcGIS Online account. | **Outside** |
| EPR-13 | `sync_eoc_situation_reports` | Posts and reads situation reports in an emergency operations platform such as WebEOC. | **Outside** |

## 8. Environmental Health and Climate (ENV)

These tools draw on EHS565 topics (water, food sanitation, vector control, housing) and the climate-health concerns raised in the PHNL program.

| ID | Tool | Description | Source |
|---|---|---|---|
| ENV-01 | `heat_health_action_planner` | Combines National Weather Service forecasts and HeatRisk with vulnerable-population data into a heat response checklist: cooling centers, outreach scripts, guidance for schools and outdoor workers, and heat illness teaching. | Live + Doc |
| ENV-02 | `air_quality_protective_guidance` | Reports current and forecast AQI from AirNow for Massachusetts reporting areas, with protective actions for sensitive groups, schools, and outdoor events. | Live |
| ENV-03 | `drinking_water_quality_lookup` | Reports violations and results for public water systems (EPA SDWIS) and MassDEP PFAS results, plus private well testing guidance for residents. | Live + Snapshot |
| ENV-04 | `environmental_justice_screen` | Combines Massachusetts Environmental Justice Population block groups with archived EPA EJScreen indicators (EPA removed the tool in 2025) and the CDC/ATSDR Environmental Justice Index where available. | Snapshot |
| ENV-05 | `climate_health_vulnerability_profile` | Builds a town profile of heat exposure, tree canopy, FEMA flood zones, social vulnerability, and climate-sensitive health outcomes, for Municipal Vulnerability Preparedness (MVP) and hazard planning. | Live + Snapshot |
| ENV-06 | `vector_borne_disease_risk` | Reports DPH mosquito-borne disease (EEE, West Nile virus) risk levels by town during the season and tick-borne disease incidence (Lyme disease, babesiosis, anaplasmosis), with prevention messages for schools and camps. | Live + Snapshot |
| ENV-07 | `housing_conditions_health_guide` | Uses the State Sanitary Code (105 CMR 410.000) to explain housing conditions that affect health (moisture and mold, pests, heat, lead) and how to request a board of health inspection. Produces tenant teaching sheets. | Snapshot + Doc |
| ENV-08 | `chemical_exposure_reference` | Looks up ATSDR ToxFAQs, minimal risk levels, and NIOSH Pocket Guide entries for community chemical exposure questions, such as spills or contaminated sites. | Snapshot |

## 9. Chronic Disease Prevention and Health Promotion (CHR)

These tools cover community screening, prevention programs, and the data needed to target chronic disease work.

| ID | Tool | Description | Source |
|---|---|---|---|
| CHR-01 | `get_places_estimates` | Pulls CDC PLACES estimates for Massachusetts counties, towns, ZIP code areas (ZCTAs), and census tracts: chronic conditions, prevention practices, disability, and health-related social needs. | Live |
| CHR-02 | `community_bp_screening_toolkit` | Provides a community blood pressure screening kit: measurement technique checklist, current ACC/AHA categories, severe-range referral thresholds, screening log, and follow-up letters. | Snapshot + Doc |
| CHR-03 | `diabetes_prevention_finder` | Scores the CDC Prediabetes Risk Test and finds CDC-recognized National Diabetes Prevention Program (DPP) programs, in person or online, that serve Massachusetts residents. | Calc + Snapshot |
| CHR-04 | `preventive_services_lookup` | Lists USPSTF A and B recommendations for a person's age, sex, pregnancy status, and risk factors, with plain-language client summaries and Massachusetts cancer screening program options. | Live + Snapshot |
| CHR-05 | `asthma_home_assessment` | Provides an EPA home asthma trigger checklist, an asthma control assessment, an asthma action plan template, and local home-based asthma services. | Snapshot + Doc |
| CHR-06 | `food_access_mapper` | Maps USDA Food Access Research Atlas low-income/low-access tracts, SNAP retailers, farmers markets that accept HIP (Healthy Incentives Program), and a local food pantry list. | Live + Snapshot + Local |
| CHR-07 | `community_water_fluoridation_status` | Reports fluoridation status by Massachusetts town (CDC *My Water's Fluoride*, DPH), with current evidence summaries for board of health discussions. | Snapshot |

## 10. Health Equity and Community Assessment (EQ)

These tools support the community and targeted needs assessments taught in N640/N698G, viewed through the HPP605 and N704 equity lens.

| ID | Tool | Description | Source |
|---|---|---|---|
| EQ-01 | `build_community_health_profile` | Builds a one-call profile of any Massachusetts city, town, county, or tract: demographics, language, income, insurance, disability, housing, chronic disease, social vulnerability, births, and deaths. Compares with state and national figures and cites sources. | Live + Snapshot |
| EQ-02 | `get_social_vulnerability_index` | Reports the CDC/ATSDR Social Vulnerability Index (SVI) by tract, with overall and theme rankings (socioeconomic status, household characteristics, racial and ethnic minority status, housing type and transportation). | Snapshot |
| EQ-03 | `language_access_profile` | Counts residents with limited English proficiency by language for a town or tract. Flags languages that meet the HHS safe-harbor threshold (5% or 1,000 people) for translating vital documents. | Live + Calc |
| EQ-04 | `measure_health_disparities` | Calculates rate ratios, rate differences, the index of disparity, between-group variance, and population attributable fractions, with confidence intervals, from public or local data. | Calc |
| EQ-05 | `screen_health_related_social_needs` | Scores the CMS Accountable Health Communities health-related social needs (HRSN) screening tool and maps positive domains (housing, food, transportation, utilities, safety) to referral categories. | Calc + Snapshot |
| EQ-06 | `estimate_benefit_eligibility` | Estimates likely eligibility for WIC, SNAP, MassHealth, the Health Safety Net, and fuel assistance from household size and income, using current federal poverty guidelines. It is a screen, not an eligibility determination. | Snapshot + Calc |
| EQ-07 | `find_community_resources` | Searches HRSA health centers and a resource directory the nurse maintains (CSV) by town, service, language, and hours. | Live + Local |
| EQ-08 | `community_health_assessment_builder` | Guides a community health assessment using MAPP 2.0. Assembles secondary data, structures the partner, status, and context assessments, pulls findings from area hospitals' community health needs assessments (CHNAs), and drafts report sections. | Live + Doc |
| EQ-09 | `prioritize_health_issues` | Ranks community health issues using the Hanlon method with the PEARL test, strategy grids, and nominal group scoring, and documents the rationale. | Calc |
| EQ-10 | `analyze_qualitative_data` | Helps analyze local focus group and key informant transcripts: removes names, drafts a codebook, builds a coded-excerpt matrix, and summarizes themes. | Local + Doc |
| EQ-11 | `newcomer_health_navigator` | Covers immigrant and refugee health: CDC domestic refugee screening guidance, DPH's refugee and immigrant health program, coverage options by immigration status, and shelter health resources. | Snapshot |
| EQ-12 | `send_closed_loop_referral` | Sends and tracks referrals through a community referral platform such as Mass 211, Findhelp, or Unite Us. | **Outside** |

## 11. Epidemiology, Biostatistics, and Data (EPI)

These shared calculation and data-handling engines are used by many other tools. They build on N540, N715, and N735.

| ID | Tool | Description | Source |
|---|---|---|---|
| EPI-01 | `calculate_rates` | Calculates crude and category-specific rates per 1,000 or 100,000, with exact Poisson confidence intervals and a warning for small numbers. | Calc |
| EPI-02 | `age_adjust_rates` | Performs direct age adjustment with the 2000 US standard population and gamma-method confidence intervals, for comparing towns or years. | Calc |
| EPI-03 | `analyze_two_by_two` | Calculates relative risk, odds ratio, attributable risk, population attributable fraction, chi-square, and Fisher's exact test, with confidence intervals. | Calc |
| EPI-04 | `calculate_sample_size` | Calculates sample size and power for surveys (with finite population correction and design effect), two-group comparisons, and cluster samples. | Calc |
| EPI-05 | `compute_standardized_ratio` | Performs indirect standardization (SIR/SMR) with exact confidence intervals, for example to answer community cancer concerns with Massachusetts Cancer Registry town data. | Calc + Snapshot |
| EPI-06 | `apply_small_cell_suppression` | Applies DPH-style small-cell suppression (configurable threshold), with complementary suppression, so tables are safe to share. Other tools call it automatically. | Calc |
| EPI-07 | `profile_local_dataset` | Describes a local CSV/XLSX file: variables, types, missing values, outliers, and possible identifiers (names, dates of birth, addresses, phone numbers, record numbers). Warns before showing anything. | Local |
| EPI-08 | `deidentify_dataset` | Writes a de-identified copy of a local dataset under the HIPAA Safe Harbor method: removes the 18 identifiers, converts dates to years or ages, and applies the ZIP code rules. Reports what changed. | Local |
| EPI-09 | `analyze_trends` | Calculates annual percent change and runs segmented (joinpoint-style) regression and Poisson or negative binomial trend tests, with seasonality handling. | Calc |
| EPI-10 | `run_statistical_test` | Picks and runs a suitable test for the question and data (t-test, chi-square, ANOVA, Mann-Whitney, correlation, linear or logistic regression), checks assumptions, and explains the result in plain language. | Local + Calc |
| EPI-11 | `create_chart` | Makes report-quality bar, line, and dot charts as SVG/PNG, with accessible colors and alt text. | Calc |
| EPI-12 | `create_choropleth_map` | Maps an indicator by Massachusetts town, tract, ZIP code area, or school district using bundled MassGIS boundaries. Respects suppression and uses quantile or natural-breaks classes. | Snapshot + Calc |
| EPI-13 | `geocode_and_aggregate_addresses` | Geocodes a local address list offline against bundled MassGIS/Census address data and returns only counts by town or tract. Addresses never leave the container or reach the AI model. | Local + Snapshot |
| EPI-14 | `expert_determination_deidentification` | De-identifies data under the HIPAA Expert Determination method, allowing richer data releases. | **Outside** |
| EPI-15 | `query_public_health_data_warehouse` | Runs analyses on the Massachusetts Public Health Data Warehouse (linked state datasets). | **Outside** |

## 12. Program Planning, Evaluation, and Grants (PLAN)

These tools mirror the N750/N798LL sequence: designing, implementing, monitoring, and evaluating population-based programs, including financial management and grant writing.

| ID | Tool | Description | Source |
|---|---|---|---|
| PLAN-01 | `build_logic_model` | Builds a logic model (inputs, activities, outputs, and short-, intermediate-, and long-term outcomes) following the CDC Program Evaluation Framework. Exports a table and a diagram. | Doc |
| PLAN-02 | `write_smartie_objectives` | Turns goals into SMARTIE objectives (specific, measurable, achievable, relevant, time-bound, inclusive, equitable) and links each to Healthy People 2030 objectives and baselines. | Live + Doc |
| PLAN-03 | `find_evidence_based_interventions` | Finds interventions for a health issue and setting from The Community Guide, County Health Rankings' *What Works for Health*, and CDC HI-5, with evidence ratings. | Snapshot + Live |
| PLAN-04 | `apply_behavior_change_theory` | Maps program components to constructs from the Health Belief Model, Transtheoretical Model, Social Cognitive Theory, Theory of Planned Behavior, Diffusion of Innovations, and the Social Ecological Model, and suggests measures. | Snapshot |
| PLAN-05 | `build_evaluation_plan` | Drafts evaluation questions, indicators, data sources, methods, and a timeline for process and outcome evaluation, with a RE-AIM option. | Doc |
| PLAN-06 | `search_grant_opportunities` | Searches Grants.gov and Massachusetts state funding notices (COMMBUYS) for opportunities open to local or state health agencies, filtered by topic and deadline. | Live |
| PLAN-07 | `analyze_funding_announcement` | Reads a federal notice of funding opportunity (NOFO) or a Massachusetts Request for Response (RFR) from the local folder. Extracts eligibility, deadlines, page limits, required attachments, and scoring criteria into a compliance checklist and outline. | Local + Doc |
| PLAN-08 | `draft_grant_narrative` | Drafts needs statements, project narratives, and sustainability sections that follow the scoring criteria, using cited data from the community profile tools. | Live + Doc |
| PLAN-09 | `build_program_budget` | Builds a line-item budget in federal object classes (personnel, fringe, travel, equipment, supplies, contractual, other, indirect) with multi-year escalation, cost share, and a budget justification. Exports XLSX. | Calc + Doc |
| PLAN-10 | `economic_evaluation_calculator` | Calculates cost per participant and per outcome, cost-effectiveness ratios, return on investment, and break-even, with one-way sensitivity analysis. | Calc |
| PLAN-11 | `track_program_performance` | Compares a local metrics file with targets, builds run charts, and drafts quarterly grant progress reports. | Local + Doc |

## 13. Leadership, Finance, and Administration (LEAD)

These tools support DNP-prepared nurse leaders who manage budgets, staff, accreditation, and the board of health. They build on N725, HPP612, and N701, and on the Massachusetts SAPHE 2.0 public health reforms.

| ID | Tool | Description | Source |
|---|---|---|---|
| LEAD-01 | `analyze_health_department_budget` | Analyzes a local budget file: year-to-date variance, revenue mix (fees, grants, Public Health Excellence funds), cost per service, and year-end forecast. | Local + Calc |
| LEAD-02 | `model_staffing_needs` | Estimates public health nurse FTEs from service volumes and time standards, compares scenarios, and shows caseload per nurse. | Local + Calc |
| LEAD-03 | `assess_foundational_public_health_services` | Runs a gap analysis against the Massachusetts local public health performance standards developed under the 2020 SAPHE 2.0 law and the Foundational Public Health Services framework, including shared-services options. | Snapshot + Calc |
| LEAD-04 | `phab_accreditation_crosswalk` | Indexes documents in the local folder against the PHAB Standards & Measures (Version 2022) and shows which measures have evidence and which have gaps. | Local + Snapshot |
| LEAD-05 | `build_strategic_plan` | Runs a SWOT or SOAR analysis and a stakeholder (power/interest) analysis. Drafts strategic priorities, goals, objectives, and strategies that meet PHAB requirements and link to the community health improvement plan (CHIP). | Doc |
| LEAD-06 | `assess_workforce_competencies` | Runs self-assessments against the Council on Linkages Core Competencies and the Community/Public Health Nursing Competencies. Builds a team training plan and tracks Massachusetts local public health credential requirements. | Calc + Doc |
| LEAD-07 | `board_of_health_meeting_assistant` | Builds agendas and staff reports, drafts minutes from notes, and checks Open Meeting Law requirements (posting 48 hours ahead, excluding weekends and legal holidays, and required minutes content). | Snapshot + Doc |
| LEAD-08 | `draft_standing_orders` | Drafts standing orders and protocols (for example from Immunize.org templates) consistent with Board of Registration in Nursing rules, ready for medical director review and signature. | Snapshot + Doc |
| LEAD-09 | `cpt_billing_assistant` | Codes and prepares insurer and MassHealth claims for billable services such as vaccine administration. | **Outside** |

## 14. Policy, Law, and Ethics (POL)

These tools build on N651's framework for the political and ethical issues that shape health care, and on N760's emphasis on political, cultural, and ethical factors in interventions.

| ID | Tool | Description | Source |
|---|---|---|---|
| POL-01 | `track_ma_legislation` | Tracks Massachusetts bills on public health topics (status, committee, hearings) using the Legislature's public data, and summarizes likely effects on local health. | Live |
| POL-02 | `search_public_health_law` | Searches M.G.L. Chapter 111, 105 CMR, 244 CMR, and local board of health regulations in the local folder, and answers with citations. | Snapshot + Local |
| POL-03 | `monitor_federal_rules` | Watches the Federal Register and Regulations.gov for CDC, CMS, HRSA, EPA, and USDA actions and comment periods, and drafts a comment outline. | Live |
| POL-04 | `draft_policy_brief` | Drafts one-page and longer policy briefs: problem, evidence, options compared against criteria (Bardach's eightfold path), and a recommendation. | Doc |
| POL-05 | `public_health_ethics_analysis` | Structures an ethics analysis using the Kass framework and the APHA Public Health Code of Ethics, for decisions such as quarantine, exclusion, or allocating scarce resources. | Snapshot + Doc |
| POL-06 | `nursing_scope_of_practice_guide` | Answers scope, delegation, and standing-order questions from 244 CMR and Board of Registration in Nursing advisory rulings, with citations. | Snapshot |
| POL-07 | `health_information_sharing_guide` | Works out which rules govern a record and what may be shared with whom: HIPAA, FERPA (school health records), 42 CFR Part 2, Massachusetts public records law, and 105 CMR 300.000 confidentiality provisions. | Snapshot |
| POL-08 | `ana_phn_scope_standards_reference` | Makes the ANA *Public Health Nursing: Scope and Standards of Practice* (2022) searchable. | **Outside** |

## 15. Health Communication and Language Access (COMM)

These tools help produce clear, accessible, multilingual communication for diverse Massachusetts communities.

| ID | Tool | Description | Source |
|---|---|---|---|
| COMM-01 | `check_readability` | Scores drafts with Flesch-Kincaid, SMOG, and the CDC Clear Communication Index. Flags jargon and long sentences and suggests plain-language fixes. | Calc |
| COMM-02 | `translate_health_material` | Drafts translations into Massachusetts' most common languages using a public health glossary and a back-translation check. Output is marked as a draft for review by a qualified translator. | Snapshot + Doc |
| COMM-03 | `create_social_media_campaign` | Builds a series of posts within platform limits, with alt text, accessibility checks, and a calendar tied to national health observances. | Snapshot + Doc |
| COMM-04 | `draft_press_materials` | Drafts press releases, media advisories, talking points, and spokesperson Q&A. | Doc |
| COMM-05 | `check_document_accessibility` | Checks DOCX, HTML, and basic PDF files for headings, alt text, color contrast, table structure, and reading order against Section 508 and WCAG 2.1 AA. | Local |
| COMM-06 | `certified_translation_workflow` | Sends vital documents to qualified human translators and tracks certification. | **Outside** |

## 16. Evidence-Based Practice and Quality Improvement (EBP)

These tools support the evidence-translation and QI skills built in the DNP project sequence and N701.

| ID | Tool | Description | Source |
|---|---|---|---|
| EBP-01 | `search_pubmed` | Builds PICO-based PubMed searches with MeSH terms, applies filters (systematic reviews, date, language), and returns abstracts and PMIDs. | Live |
| EBP-02 | `search_cdc_guidance` | Searches MMWR, Health Alert Network advisories, and CDC Stacks for current guidance on a topic, newest first. | Live |
| EBP-03 | `appraise_evidence` | Guides critical appraisal with CASP checklists, assigns a level of evidence, and summarizes certainty in GRADE terms. | Snapshot + Doc |
| EBP-04 | `build_evidence_synthesis_table` | Builds a synthesis matrix (author, design, sample, findings, level), PRISMA 2020 flow counts, and APA 7 references from PMIDs or DOIs. | Live + Doc |
| EBP-05 | `plan_quality_improvement` | Produces Model for Improvement aim statements, PDSA cycle plans, driver diagrams, fishbone diagrams, and process maps. | Doc |
| EBP-06 | `create_spc_chart` | Builds run charts with run rules and statistical process control charts (XmR, p, u) with special-cause detection. | Local + Calc |
| EBP-07 | `qi_vs_research_determination` | Applies the OHRP decision charts to decide whether a project is quality improvement, program evaluation, or human subjects research, and drafts the IRB determination request. | Snapshot + Doc |
| EBP-08 | `search_cinahl` | Searches CINAHL and retrieves full-text articles. | **Outside** |

## 17. PHN Practice Toolkit and Platform Utilities (CORE)

These cover public health nursing classification systems, documentation, and the platform utilities that every other section depends on.

| ID | Tool | Description | Source |
|---|---|---|---|
| CORE-01 | `omaha_system_coder` | Maps a visit narrative or care plan to Omaha System problems, signs and symptoms, interventions (category and target), and Knowledge-Behavior-Status ratings. The Omaha System is in the public domain. | Snapshot |
| CORE-02 | `classify_intervention_wheel` | Classifies activities by the 17 interventions of the Public Health Intervention Wheel and its three levels of practice (individual/family, community, systems), for reporting and workload analysis. | Snapshot |
| CORE-03 | `nursing_documentation_template` | Provides SOAP, DAR, and narrative templates for home visits, clinics, and case investigations, and checks for required elements (consent, teaching, follow-up). | Doc |
| CORE-04 | `read_local_file` | Lists files in the mounted folder and extracts text and tables from PDF, DOCX, XLSX, and CSV files. Warns when identifiers are detected. | Local |
| CORE-05 | `search_agency_policies` | Indexes the agency's own policies and procedures in the local folder and answers "what does our policy say?" with file and section citations. | Local |
| CORE-06 | `export_document` | Exports tool output to DOCX, PDF, XLSX, CSV, or PNG/SVG using an agency letterhead template, saved in the output folder. | Doc |
| CORE-07 | `lookup_medication_information` | Looks up FDA labeling (DailyMed, openFDA), RxNorm names, drug shortages, and recalls. For reference only. | Live |
| CORE-08 | `get_data_source_status` | Lists every data source with live or offline status, snapshot date, and known caveats (for example, federal datasets removed or changed since 2025). | Calc |
| CORE-09 | `configure_jurisdiction` | Sets the nurse's town and county, HMCC region, school district, suppression threshold, preferred languages, enabled toolsets, and letterhead. | Local |
| CORE-10 | `check_drug_interactions` | Checks for drug–drug interactions. | **Outside** |
| CORE-11 | `search_licensed_clinical_reference` | Searches subscription clinical references such as UpToDate or Lexicomp. | **Outside** |
