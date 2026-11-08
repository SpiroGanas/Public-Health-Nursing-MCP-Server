# Acceptance script: Phase 6 (immunization forecasting and surveillance feeds)

Run these prompts in Claude Desktop with the `vax` and `cd` toolsets enabled (for example
`PHN_PROFILE=cd_nurse`). Use fictional dose histories only. Record pass/fail and notes.

The forecaster is checked automatically against the CDC CDSi test cases; see
[`docs/cdsi-test-results.md`](../cdsi-test-results.md) (all 1,064 healthy childhood and adult
cases pass; 36 underlying-condition cases are documented exceptions for review).

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "A 15-month-old (born 2025-08-02) had Pediarix at 2, 4, and 6 months and nothing since. What's due today?" | `forecast_immunizations_due` | Valid doses; MMR, varicella, HepA, Hib, PCV due with earliest/recommended/past-due dates; ages shown, not the birth date; MIIS and ACIP-vs-DPH caveats. |
| 2 | "Same child, but the 4-month dose was given 3 weeks after the first." | `forecast_immunizations_due` | That dose is invalid (minimum interval) with the reason; catch-up schedule adjusts. |
| 3 | "A 13-year-old girl with one HPV dose at age 12 and Tdap at 11 — anything due?" | `forecast_immunizations_due` | HPV dose 2 timing; MenACWY. |
| 4 | "Forecast everyone in immunization_history.csv." | `forecast_immunizations_due` (file mode) | Per-person results written to /data/output; the reply has only counts due and overdue by vaccine, with suppression. |
| 5 | "How does pertussis in Massachusetts this year compare with the past 5 years?" | `get_notifiable_disease_trends` | Year-to-date and last 4 weeks vs the 5-year mean and range for MA, New England, U.S.; provisional-data caveat. |
| 6 | "Give me an overview of notifiable diseases for the latest week." | `get_notifiable_disease_trends` | Overview table of conditions; method stated. |
| 7 | "Write my weekly respiratory and wastewater briefing." | `get_respiratory_and_wastewater_surveillance` | COVID-19, flu, RSV levels and trends; NWSS sites; each source's latest week and lag; DPH dashboard linked; MWRA data marked as ending July 2026. |
