# Acceptance script: Phase 4 (public data and evidence platform)

Run these prompts in Claude Desktop with the `equity`, `chronic`, `epi`, and `ebp` toolsets
enabled (for example `PHN_PROFILE=phn_leader`). Run the first five twice: once online and once
with `PHN_OFFLINE=1`, which must answer from the bundled snapshots and say so. Record pass/fail
and notes.

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Build a community health profile for Chelsea and compare it with Massachusetts and the U.S." | `build_community_health_profile` | Demographics, language, income, insurance, disability, housing, PLACES, SVI; every figure cites its ACS table and vintage; estimates with CV > 30% flagged. |
| 2 | "Same for Gosnold." (a town with no census tract of its own) | `build_community_health_profile` | Works; SVI uses overlapping tracts with a caveat. |
| 3 | "Which census tracts in Springfield are most socially vulnerable, and why?" | `get_social_vulnerability_index` | Tracts ranked with theme percentiles; Massachusetts ranking by default. |
| 4 | "Which languages do we need to translate vital documents into for Lowell's residents?" | `language_access_profile` | Languages meeting 5% or 1,000 LEP persons; the HHS 2003 safe-harbor entry is marked low confidence pending review. |
| 5 | "What do CDC PLACES estimates say about diabetes and obesity in ZIP 01105?" | `get_places_estimates` | Estimates with CIs, data year vs release year, MA and U.S. comparison, the "not for evaluating local programs" caveat. |
| 6 | "Hospitalization rates by race/ethnicity: [counts and populations]. How large are the disparities?" | `measure_health_disparities` | Rate ratios and differences vs the most favorable group with CIs, index of disparity, between-group variance, excess cases. |
| 7 | "Map these town rates for Hampden County: [values]." | `create_choropleth_map` | PNG/SVG in output/maps; natural-breaks classes; legend; alt text; small counts hatched when `value_is_count`. |
| 8 | "Map counts_by_town.csv." | `create_choropleth_map` | Small counts suppressed before mapping; row-level files refused. |
| 9 | "Find recent systematic reviews on school-located influenza vaccination and absenteeism." | `search_pubmed` | PMIDs, titles, abstract excerpts, the exact query with MeSH terms. |
| 10 | "What has CDC published lately about measles?" | `search_cdc_guidance` | MMWR and HAN items newest first; Stacks offered as a search link. |
