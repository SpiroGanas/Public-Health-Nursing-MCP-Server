# Acceptance script: Phase 2 (epidemiology and biostatistics engine)

Run these prompts in Claude Desktop with the synthetic corpus (`fixtures/synthetic/data`) in the
mounted folder and the `epi` and `ebp` toolsets enabled (for example `PHN_PROFILE=phn_leader` or
`PHN_TOOLSETS=epi,ebp`). Record pass/fail and notes. Every calculation is also checked
automatically against R, SciPy, or published tables (`crates/phn-epi/tests/golden.rs`); this
script checks that the answers are useful and correctly explained.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "At the church supper, 30 of 40 people who ate potato salad got sick versus 5 of 30 who didn't. Is the salad the likely vehicle?" | `analyze_two_by_two` | Attack rates 75% vs 16.7%; RR 4.5 (about 2.0–10.2); Fisher p < 0.001; causation caveat. |
| 2 | "Repeat that stratified by age: children 20/100 vs 10/100, adults 40/100 vs 25/100, as a case-control study." | `analyze_two_by_two` | MH odds ratio, Breslow–Day test, no risk ratio for case-control. |
| 3 | "Age-adjust these two towns' hospitalization rates and compare them" (give counts and populations for 0–17, 18–44, 45–64, 65+). | `age_adjust_rates` | 2000 U.S. standard; NCHS weights; gamma CIs; rate ratio vs reference; <20-event flags. |
| 4 | "Our town had 18 bladder cancers in 2015–2019; here are our populations and the state rates by age. Is that more than expected?" | `compute_standardized_ratio` | SIR with exact CI and p; note about comparing with the state only; cluster-guidance caveat. |
| 5 | "How many kindergarten records do I need to review to estimate MMR coverage within ±5% in a district of 1,000 students?" | `calculate_sample_size` | 278; formula and assumptions listed. |
| 6 | "How many cases and controls to detect an odds ratio of 2 if 30% of controls are exposed, 2 controls per case, 80% response?" | `calculate_sample_size` | Cases and controls with continuity correction, inflated for response. |
| 7 | "Here are Lyme disease counts and populations for 2012–2025. Is it going up? Did the trend change?" | `analyze_trends` | APC with CI; joinpoint segments and AAPC; chart saved; BIC/conditional-CI caveat. |
| 8 | "Monthly ED visits for asthma for 3 years — adjust for season. Did the smoke-free housing policy in March 2025 change anything?" | `analyze_trends` | Interrupted time series (level and slope change) with seasonality. |
| 9 | "Did wait times differ between our two flu clinics?" (give the minutes) | `run_statistical_test` | Picks Welch t or Mann–Whitney with the reason; assumption checks; plain-language result. |
| 10 | "In line_list.csv, is age different between hospitalized and non-hospitalized cases?" | `run_statistical_test` | Group n, mean, SD, median, IQR only — no names, no minimum or maximum. |
| 11 | "In line_list.csv, compare age by last name." | `run_statistical_test` | Refused: the column contains names. |
| 12 | "Make a bar chart of pertussis cases by town: Amherst 12, Hadley 3, Pelham 7, and suppress small counts." | `create_chart` | PNG/SVG in output/charts; Hadley hatched; alt text; embed markdown. |
| 13 | "Put that chart in a one-page report as a Word document." | `export_document` | The image appears with its alt text; accessibility check passes. |
| 14 | "Make a p-chart of the monthly percentage of doses documented within 24 hours" (12 months of numerators and denominators). | `create_spc_chart` | Limits vary with the denominators; the low month is flagged as special cause. |
| 15 | "Make a run chart of line_list.csv cases by week." | `create_spc_chart` | Weekly counts with small counts hidden in the reply; chart saved locally. |
