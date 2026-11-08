# Tool reference

Every tool returns the same envelope:

- **Text content 1:** a markdown summary written for the model, followed by caveats, sources,
  and a privacy line.
- **Text content 2 and `structuredContent`:** the full envelope as JSON — `summary`, `data`
  (tool-specific, described below), `sources[]` (title, publisher, URL, effective and retrieval
  dates, and `mode`: `live`, `cache`, `snapshot`, `local`, or `computed`), `caveats[]`,
  `privacy` (`row_level_data`, `suppression_applied`, `suppression_threshold`,
  `identifiers_detected`, `files_written`, `notes`), and an optional decision-support `notice`.

Each tool publishes JSON Schemas for its input and output, and MCP annotations
(`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`). Problems the nurse can
fix (a misspelled town, a missing file) come back as tool results with `isError: true` and a
plain-language message.

Paths are relative to the mounted data folder (`/data`); `/data/...` is also accepted.

---

## Phase 0

### `get_data_source_status` (CORE-08, toolset `core`)

Lists every data source the server uses: bundled reference content (with its review status and
due date), dated snapshots (with source vintage, build date, and whether a newer cached copy is
in use), the nurse's files, and, with `include_planned`, sources planned for later phases.

| Argument | Default | |
|---|---|---|
| `check_live` | `false` | Probe each live endpoint now (HEAD, then GET). Results are cached in `/cache`. |
| `include_planned` | `false` | Include sources planned for later phases. |
| `source_id` | all | Report one source. |

Read-only; open-world (it can call public endpoints). Offline mode reports every endpoint as
unreachable without calling it.

### `configure_jurisdiction` (CORE-09, toolset `core`)

Views or updates settings saved in `/cache/phn-settings.toml`. Call with no arguments to see the
current settings.

| Argument | |
|---|---|
| `town` | One of the 351 Massachusetts municipalities (misspellings get suggestions). Sets county and HMCC region. |
| `school_district` | District name or 8-digit DESE organization code. |
| `hmcc_region` | Override: `1`, `2`, `3`, `4AB`, `4C`, or `5`. |
| `board_of_health_phone` | Shown by `lookup_reportable_condition`. |
| `suppression_threshold` | 2–30 (default 5). |
| `preferred_languages` | List, most important first. |
| `profile` / `toolsets` | Role profile (or `"none"`) and explicit toolsets. Changing them refreshes the tool list. |
| `letterhead` | `agency_name` (required), `department`, `address_lines`, `phone`, `fax`, `email`, `website`, `logo_path` (PNG/JPEG in `/data`), `footer_text`. |
| `reset` | Clear all settings first. |

Environment variables in the client configuration override saved values; the result says when
that happens. HMCC regions are derived from the 2005 DPH preparedness-region layer (4A and 4B
merged into 4AB), corrected where DPH's HMCC Regional Boundaries map (December 2019) differs
(Wilmington, Region 3), and carry that caveat.

### `calculate_rates` (EPI-01, toolset `epi`)

| Argument | Default | |
|---|---|---|
| `groups` | required | `[{label, count, population}]`, up to 500. |
| `per` | 100,000 | Rate base. |
| `confidence` | 0.95 | 0.80–0.999. |
| `include_total` | true for 2+ groups | Crude total across groups. |
| `event_description` | | Label for the summary. |

Each rate has an exact (Garwood) Poisson interval, the relative standard error, and flags:
`zero_count`, `unreliable` (< 20 events), `below_suppression_threshold`,
`count_exceeds_denominator`. Golden tests compare the intervals with SciPy to 10⁻⁷.

### `lookup_reportable_condition` (CD-01, toolset `cd`)

| Argument | |
|---|---|
| `query` | Disease, organism, or lab result ("whooping cough", "Salmonella Typhi", "positive HBsAg"). |
| `max_alternatives` | Other candidates to list (default 3). |

Returns whether the condition is reportable under 105 CMR 300.000, the timeframe (immediately
by phone, within 24 hours, or a program-specific channel), who reports, to whom, how, the
regulation sections the entry was transcribed from, and the chapter of DPH's *Guide to
Surveillance, Reporting and Control* when the current handbook has one. Includes the nurse's own
board of health when configured and the DPH Epidemiology line. When nothing matches, it says that
this does **not** mean the condition is not reportable: clusters and unusual illnesses always are.

The content was re-checked on 2026-11-07 against the regulation text (Mass. Register #1470,
5/27/22) and DPH's current provider and laboratory reporting lists, and it remains pending
subject-matter review. Each entry cites its sections and has a confidence level; low-confidence
entries carry an extra caveat.

## Phase 1

### `read_local_file` (CORE-04, toolset `core`)

| Argument | Default | |
|---|---|---|
| `action` | `read` for a file, `list` for a folder | |
| `path` | top folder | |
| `recursive` | false | For `list`. |
| `sheet` | all sheets | Spreadsheets. |
| `include_rows` | false | CSV/XLSX: include masked sample rows. |
| `max_rows` | 20 | 1–200. |
| `max_chars` | 20,000 | Document text, up to 100,000. |
| `redaction` | `standard` | `standard` or `strict`. |

**Privacy:** for CSV/XLSX, only sheet names, columns, row counts, and possible identifiers are
returned by default. With `include_rows`, identifier columns are replaced by
`[REDACTED: kind]`, dates by year, ZIP codes by three digits, ages over 89 by 90+, and
identifiers inside other text are redacted. For documents, standard redaction removes Social
Security numbers and labeled dates of birth and record, member, account, and license numbers;
if any of those are found the whole document is redacted strictly (names, phone numbers,
emails, addresses, and dates too). There is no way to turn redaction off.

### `search_agency_policies` (CORE-05, toolset `core`)

| Argument | Default | |
|---|---|---|
| `query` | required | Words, "quoted phrases", AND/OR/NOT. |
| `folder` | `/data/policies` | |
| `max_results` | 5 | 1–20. |

Indexes PDF (by page), DOCX (by heading), Markdown (by heading), text, and HTML with `tantivy`
in `/cache/index`, rebuilding automatically when files change. Returns passages with query
terms in bold and citations such as `Exposure Control Plan.docx, § Reporting` or
`TB policy.pdf, p. 4`.

### `export_document` (CORE-06, toolset `core`)

| Argument | Default | |
|---|---|---|
| `title`, `subtitle` | | Title is also the document's title property. |
| `content_markdown` | required | Headings, paragraphs, bold/italic, lists, links, pipe tables (`Table: caption` line before a table), images `![alt](path)`, `\pagebreak`. |
| `formats` | `["docx"]` | `docx`, `pdf`, `xlsx`, `csv`, `svg`, `png`. |
| `file_name` | from title | Files are never overwritten (a number is added). |
| `language` | `en-US` | BCP 47 tag set on the document. |
| `use_letterhead` | true when configured | |
| `table_index` | | Which table for CSV. |
| `merge_data_path`, `merge_sheet`, `merge_mode` | | Local mail-merge from a CSV/XLSX: `{{Column}}` placeholders; `combined` (default) or `one_file_per_record`. |

DOCX output uses real heading styles, a document title and language, repeating table header
rows with captions, alt text on images, and a letterhead header with page numbers. PDF output is
tagged (headings, paragraphs, lists, tables with column-scoped headers, figures with alt text),
embeds subset DejaVu fonts with Unicode maps, sets the language and title, and marks the
letterhead and page numbers as artifacts. Every DOCX and PDF is checked with
`check_document_accessibility` before the result is returned. Images must have alt text.

**Privacy:** merged documents are written to `/data/output`; the response contains only counts,
field names, and generic file names.

### `apply_small_cell_suppression` (EPI-06, toolset `epi`)

| Argument | Default | |
|---|---|---|
| `table` or `path` | | Inline `{columns, rows}` or a CSV/XLSX of counts. |
| `label_columns` | detected | Leading text columns. |
| `threshold` | configured (5) | |
| `suppress_zeros` | false | |
| `complementary` | true | |
| `last_column_is_total`, `last_row_is_total` | detected from "Total" | |
| `marker` | `*` | Same marker for primary and complementary cells. |
| `write_output` | true for files | Writes `<name>_suppressed.csv` with a footnote. |

Complementary suppression hides additional cells until no row or column contains exactly one
hidden cell, preferring the smallest non-zero interior cells and touching totals only as a last
resort. Tables with direct identifiers are refused as record-level data.

### `profile_local_dataset` (EPI-07, toolset `epi`)

| Argument | Default | |
|---|---|---|
| `path`, `sheet` | | |
| `include_value_summaries` | false | |
| `max_categories` | 15 | |

By default: rows, columns, duplicates, inferred types, missingness, distinct counts, and possible
identifiers with the evidence (never values). With value summaries: mean, SD, quartiles, and
outlier counts for numeric columns (min/max shown only with enough records, ages over 89 as
90+), month ranges for dates other than birth dates, and frequency tables with small-cell and
complementary suppression for categorical, town, and ZIP columns. Identifier columns never
show values.

### `deidentify_dataset` (EPI-08, toolset `epi`)

| Argument | Default | |
|---|---|---|
| `path`, `sheet` | | |
| `mode` | `safe_harbor` | or `limited_data_set`. |
| `birth_date` | `age` | Replace dates of birth with age, or `year`. |
| `reference_date` | today | For ages. |
| `keep_columns` | | Keep unchanged (output is then marked NOT de-identified). |
| `drop_columns` | | |
| `replace_ids_with_codes` | true | New sequential codes; no crosswalk is kept. |
| `scrub_free_text` | false | Keep free-text columns after strict redaction instead of dropping them. |
| `output` | `csv` | or `xlsx`. |

Safe Harbor: drops names, addresses, town/county, contact numbers, SSNs, and record, plan,
account, license, vehicle, and device numbers, URLs, IP addresses, photos, and biometrics;
reduces dates to year; replaces dates of birth with age (90+ for 90 and older); reduces ZIP
codes to three digits (000 for the 17 low-population prefixes in HHS guidance); top-codes ages
over 89; redacts identifiers found in other text cells; and reports records whose combination of
quasi-identifiers is shared by fewer than *k* records (*k* = the suppression threshold).
Writes the data and a markdown report to `/data/output`; only the report is returned.

### `check_document_accessibility` (COMM-05, toolset `comms`)

| Argument | |
|---|---|
| `path` | DOCX, HTML, or PDF in `/data`. |

Checks, with the WCAG 2.1 success criterion for each: title (2.4.2), language (3.1.1), headings
present (2.4.6) and in order (1.3.1), bold paragraphs posing as headings, alt text (1.1.1),
table header rows and merged cells (1.3.1), color contrast (1.4.3), link text (2.4.4), empty
paragraphs and typed bullets, form labels (HTML, 3.3.2), tagging, title display, scanned pages,
figure alt text, and embedded fonts (PDF). Lists failures with how to fix them in Word or
Acrobat, plus the manual checks automation cannot do (reading order, use of color, alt-text
quality).

---

## Phase 2: Epidemiology and biostatistics engine

<!-- Phase 2 tool sections go below this line, one `### \`tool_name\` (PLAN-ID, toolset \`x\`)` section per tool. -->

### `age_adjust_rates` (EPI-02, toolset `epi`)

Directly age-adjusts rates for one or more groups (towns, years, counties).

| Argument | Default | |
|---|---|---|
| `groups` | | Each with a `label` and `strata`: `age_group` ("<1", "1-4", "45–54", "85+", "65 and over"), `count`, `population`. |
| `custom_standard` | 2000 U.S. standard | `age_group` and `population` pairs. |
| `per` | 100,000 | Rate base. |
| `confidence` | 0.95 | |
| `reference_group` | | Compare every other group with this one (rate ratio and difference, z-test). |
| `event_description` | | |

Method: Σ wᵢ·(countᵢ/populationᵢ) with the 2000 U.S. standard population. Weights come from the
NCHS Master List (Healthy People 2010 Statistical Notes No. 20) when the age groups line up with
it — reproducing NCHS weights exactly — and from the single-year P25-1130 population (NCI SEER)
otherwise. Confidence intervals use the gamma method of Fay and Feuer (1997); crude rates use
exact Poisson intervals. Flags rates from fewer than 20 events and totals below the suppression
threshold. Golden tests: R `epitools::ageadjust.direct`.

### `analyze_two_by_two` (EPI-03, toolset `epi`)

| Argument | Default | |
|---|---|---|
| `tables` | | One table, or one per stratum: `exposed_cases`, `exposed_noncases`, `unexposed_cases`, `unexposed_noncases`, optional `label`. |
| `design` | `cohort` | `cohort`, `case_control`, or `cross_sectional`. |
| `confidence` | 0.95 | |
| `exposure`, `outcome` | | Labels for the interpretation. |

Returns attack rates (or prevalence), risk ratio (Katz interval), risk difference (Wald), odds
ratio (Woolf), conditional MLE odds ratio with Fisher exact and mid-p intervals, attributable and
preventable fractions (Levin; Miettinen for case-control), Pearson, Yates, and Mantel–Haenszel
chi-square, Fisher's exact (two- and one-sided), and the mid-p. With two or more strata: crude and
Mantel–Haenszel odds and risk ratios (Robins–Breslow–Greenland and Greenland–Robins intervals),
the MH test, the Breslow–Day test with Tarone's correction, and the crude-versus-adjusted change
(a confounding screen). Golden tests: R `fisher.test`, `chisq.test`, `mantelhaen.test`,
`epitools`; SciPy; statsmodels.

### `calculate_sample_size` (EPI-04, toolset `epi`)

| `study_type` | Required arguments |
|---|---|
| `survey_proportion` | `expected_proportion` (default 0.5), `margin_of_error`; optional `population_size` |
| `survey_mean` | `standard_deviation`, `margin_of_error`; optional `population_size` |
| `two_proportions` | `p1` and `p2`, or `relative_risk` and `p2` |
| `case_control` | `odds_ratio`, `control_exposure` |
| `two_means` | `difference`, `standard_deviation` |
| `power_two_proportions` | `p1`, `p2`, `n1` (`n2`) |
| `power_two_means` | `difference`, `standard_deviation`, `n1` (`n2`) |

Common options: `confidence` (surveys, 0.95), `alpha` (0.05), `power` (0.80), `ratio` (group 2 ÷
group 1), `one_sided`, `continuity_correction` (true), `design_effect` or `cluster_size` + `icc`,
`response_rate`. Proportions may be given as 0–1 or percentages. Formulas: CDC Epi Info StatCalc
population survey (reproduces Krejcie & Morgan's table); Fleiss, Levin & Paik (2003) for two
proportions with unequal groups and the continuity correction; normal approximation plus
Guenther's correction for two means. Golden tests: R `power.prop.test`, `power.t.test`.

### `compute_standardized_ratio` (EPI-05, toolset `epi`)

| Argument | |
|---|---|
| `areas` | Each with `label`, `observed_total` (or per-stratum `observed`), and `strata`: `stratum`, local `population`, and `reference_rate` (per `per`) or `reference_count` + `reference_population`. |
| `per` | Rate base of `reference_rate` (default 100,000). |
| `measure` | `incidence` (SIR, default) or `mortality` (SMR). |
| `reference_name`, `event_description`, `confidence` | |

Expected = Σ population × reference rate; ratio = observed ÷ expected with the exact Poisson
interval and the exact two-sided p-value of R `poisson.test`. Notes that ratios compare each area
with the reference, not areas with each other, and points to CDC/CSTE cluster guidance.

### `analyze_trends` (EPI-09, toolset `epi`)

| Argument | Default | |
|---|---|---|
| `periods` | | In order: `label`, `count`, optional `population`. |
| `time_unit` | `year` | `quarter`, `month`, `week`. |
| `model` | `auto` | Poisson, switching to negative binomial when Pearson dispersion > 1.5. |
| `seasonal` | true for months and weeks | One annual harmonic, tested by likelihood ratio. |
| `max_joinpoints` | 3 | Limited to ⌊(n − 2)/5⌋ as in NCI Joinpoint guidance. |
| `interruptions` | | Labels of the first period after a known change (interrupted time series). |
| `per`, `confidence`, `event_description`, `chart` (true) | | |

Reports the percent change per year from one log-linear trend; the BIC-selected joinpoint model
(segment APCs and the AAPC; intervals conditional on joinpoint locations); level and slope changes
at interruptions; seasonality; Mann–Kendall τ; and Durbin–Watson autocorrelation warnings. Saves a
line chart of observed and modeled values. Golden tests: R `glm`, `MASS::glm.nb`,
`trend::mk.test`, `trend::sens.slope`.

### `run_statistical_test` (EPI-10, toolset `epi`)

Give one kind of data: `groups` (name + values), `paired` (`before`, `after`), `table`
(`row_labels`, `column_labels`, `counts`), `x` and `y`, or a local file: `path` (+ `sheet`) with
`outcome` and `group`, `paired_with`, or `predictors` (regression; `outcome_event` for logistic).
`test` defaults to `auto`; `confidence` 0.95.

Automatic choice: two groups → Welch t (or Mann–Whitney if not normal: Shapiro–Wilk p < 0.05
with n < 30); 3+ groups → ANOVA, Welch ANOVA (unequal variances by the median-centered Levene
(Brown–Forsythe) test), or
Kruskal–Wallis; paired → paired t or Wilcoxon signed-rank; counts → chi-square or Fisher's exact
(2×2 with an expected count < 5); two measurements → Pearson or Spearman; predictors → linear or
logistic regression. Alternatives run as sensitivity checks. Local files: only aggregates (n,
mean, SD, median, IQR — never minimum, maximum, or rows); identifier columns are refused; groups
smaller than the suppression threshold are refused; table cells are suppressed with complementary
suppression. Golden tests: R `t.test`, `wilcox.test`, `oneway.test`, `kruskal.test`,
`chisq.test`, `cor.test`, `shapiro.test`, `glm`; SciPy for the median-centered Levene test (tool output calls it "Levene" because the
eponym "Brown" collides with the surname detector in the leak tests).

### `create_chart` (EPI-11, toolset `epi`)

| Argument | |
|---|---|
| `chart_type` | `bar`, `stacked_bar`, `line`, `dot`, `histogram`. |
| `title` (required), `subtitle`, `x_label`, `y_label`, `source_note` | |
| `categories`, `series` | Up to 8 series: `name`, `values` (null = missing), optional `lower`, `upper`, `suppressed`. |
| `reference_lines`, `markers` | Horizontal lines (`label`, `value`) and vertical markers (`at_category`, `label`). |
| `log_scale`, `include_zero`, `decimals`, `width`, `height` | Bars always start at zero. |
| `suppress_small_counts` | Hide counts from 1 to threshold − 1 (hatched). |
| `file_name`, `format` | `png`, `svg`, or `both` (default). |

Writes to `/data/output/charts/`. Colors follow a fixed order validated for color-vision
deficiency on white; series also get marker shapes and direct labels, suppressed values are
hatched, and the SVG has a title and generated description. Returns the alt text, the data table,
and markdown to embed the image with `export_document`.

### `create_spc_chart` (EBP-06, toolset `ebp`)

| Argument | |
|---|---|
| `chart_type` | `run` (default for inline data), `xmr`, `p`, `u`, `c`. |
| `labels`, `values`, `denominators` | Inline data (`denominators` for p and u charts). |
| `path`, `sheet`, `date_column`, `period` (`week`, `month`, `quarter`), `measure` (`count`, `proportion` + `flag_column`, `mean` + `value_column`) | A local line list aggregated by period. |
| `baseline_points`, `phase_starts` | Limits from the first k points; recompute after a change. |
| `title`, `measure_name`, `display_scale` (100 for percent), `format` | |

Run charts use the median and the Anhøj rules (longest run > round(log₂ n + 3); crossings below
the 5th percentile of Binomial(n − 1, ½)). Control charts use 3-sigma limits (XmR: 2.66 × average
moving range) and the *Health Care Data Guide* rules: a point outside the limits, 8 in a row on
one side, 6 increasing or decreasing, 2 of 3 beyond 2σ, 15 within 1σ. For local files, values
from fewer records than the suppression threshold are hidden in the response (the saved chart
keeps them, locally). Golden tests: R `qcc` limits.


---

## Phase 3: Communicable disease I

<!-- Phase 3 tool sections go below this line, one `### \`tool_name\` (PLAN-ID, toolset \`x\`)` section per tool. -->

All Phase 3 reference content (disease rules, measles exposure, infection control, and the DPH
guide corpus) is **pending review** by the nurse; answers built on it say so. The rules cover 37
diseases with a citation and a confidence level per rule (`docs/acceptance/phase-3.md` has the
review checklist). The `investigate_outbreak` MCP prompt walks through an outbreak investigation
with these tools (see the end of this section).

### `search_ma_disease_control_guide` (CD-02, toolset `cd`)

| Argument | Default | |
|---|---|---|
| `query` | | Words, "quoted phrases", AND/OR/NOT. |
| `disease` | | Limit to documents about one disease (aliases work: "whooping cough"). With no query, returns the chapter outline. |
| `kind` | all | `guide_chapter`, `fact_sheet`, or `guidance`. |
| `limit` | 5 | Passages, at most 20. |
| `full_sections` | false | Return the whole section (about 2,500 characters at most) instead of an excerpt. |
| `list_documents` | false | List the corpus (titles, kinds, file dates, URLs). |

Searches a bundled plain-text corpus (`snapshots/ma_dph_guide_corpus.txt`, 1.1 MB) of 25
chapters of DPH's *Guide to Surveillance, Reporting and Control* and 66 DPH fact sheet and
guidance pages, built by `cargo xtask build-guide-corpus` from mass.gov (each document's URL,
file date, and SHA-256 are in `snapshots/manifest.json`). The tantivy index is built at first use
in `/cache/index/dph-guide` (or in memory) and is not committed. Passages are quoted verbatim with
the section path ("Section 4: Controlling Further Spread › A. Isolation and Quarantine
Requirements"), the document's file date, and its URL. The Streptococcus pneumoniae chapter (a
legacy .doc file) is not included. Decision support.

### `calculate_isolation_and_exclusion` (CD-03, toolset `cd`)

| Argument | Default | |
|---|---|---|
| `disease` | (required) | Name or alias, e.g. "pertussis", "E. coli O157", "norovirus". |
| `role` | `case` | `case` or `contact`. |
| `settings` | all five | `child_care`, `school`, `food_handler`, `health_care_worker`, `other`. |
| `onset_date`, `positive_specimen_date` | | The person's onset (rash, cough, gland swelling, symptoms) and specimen dates. |
| `first_exposure_date`, `last_exposure_date` | | For contacts (one date alone means a single exposure). |
| `case_onset_date`, `last_case_onset_date` | | The index case's onset; the last case's onset (continuous exposure). |
| `treatment_start_date`, `symptom_resolution_date` | | |
| `symptomatic`, `susceptible`, `received_prophylaxis`, `received_immune_globulin`, `outbreak`, `exposure_continuous`, `lesions_present`, `diarrhea` | unknown | Facts that decide which rules apply; unknown facts show the dependent rules as "only if". |
| `as_of` | | Report whether the person is still excluded that day. |

Rules come from `content/cd/disease_rules.yaml` (105 CMR 300.200 quoted verbatim where
possible, DPH guide chapters, DPH respiratory and GI guidance, CDC). Day counting: day 0 is the
anchor date; "through day N" means back on day N + 1; for N days of treatment the first full day
is day 1, so back on start + N; "H hours after" with dates only gives the earliest calendar date.
A more specific rule that fully applies replaces a general one (for example continuous measles
exposure). Tuberculosis is not computed (refer). Decision support.

### `analyze_outbreak_line_list` (CD-04, toolset `cd`)

| Argument | Default | |
|---|---|---|
| `path`, `sheet` | (required) | Line list in `/data`: ill and, for attack rates, well people. |
| `onset_date_column`, `onset_time_column`, `case_column`, `age_column`, `sex_column`, `town_column` | found by name | |
| `case_values` | confirmed, probable (or yes) | Values of the case column that count as cases. |
| `require_any_symptom`, `onset_from`, `onset_to` | | Case definition filters. |
| `exposure_columns` | every yes/no column that is not a symptom or outcome | |
| `disease` or `incubation_min_hours` / `incubation_typical_hours` / `incubation_max_hours` | | For the exposure window. |
| `exposure_datetime` | | A known meal or event, checked against the window. |
| `design` | `cohort` | `case_control` gives odds ratios only. |

Returns counts of cases, non-cases, and records excluded by the case definition; person (age
group, sex), place (town), time (onset date), and symptom tables with small-cell suppression
(`*`, including complementary cells); exposure-specific attack rates with risk ratios (Katz),
odds ratios (Woolf), and Fisher exact p-values from `phn_epi::two_by_two`; likely vehicles (p <
0.05 and lower confidence limit above 1); and the probable exposure window (last onset − maximum
incubation to first onset − minimum incubation; median onset − typical incubation). Identifier
columns are never used, and no row-level values are returned. In the exposure table, small counts
are masked as `<5`, but the ratios are computed from all records (the summary says so).

### `generate_epi_curve` (CD-05, toolset `cd`)

| Argument | Default | |
|---|---|---|
| `path` (+ column options as above) or `onset_counts` | | A line list, or `[{onset, count, stratum}]`. |
| `stratify_by` | | A category column; at most 8 strata (the rest become "Other"). |
| `disease` or incubation hours | | Sets the interval and markers. |
| `interval_hours` | about 1/4 of the typical incubation | From 1 hour (with onset times) to weeks. |
| `exposure_datetime` | | Marks the exposure, the expected onset range, and the typical incubation. |
| `title`, `format` | | `png`, `svg`, or `both`. |

Writes the chart to `/data/output/charts/` (exact counts, alt text, data table) and returns the
counts per interval with suppression. Describes the pattern — point source (onsets within about
1.5 × the incubation range, one peak), continuous common source, intermittent common source, or
propagated (peaks about one incubation period apart) — with its reasoning and uncertainty (CDC
Field Epidemiology Manual, chapter 6; CDC Quick-Learn "Create an Epi Curve").

### `plan_contact_tracing` (CD-06, toolset `cd`)

| Argument | Default | |
|---|---|---|
| `disease` | (required) | |
| `onset_date`, `positive_specimen_date`, `treatment_start_date` | | Index case dates. |
| `isolation_start_date` | | Ends the contact elicitation window early. |
| `write_template`, `template_rows` | true, 50 | |

Returns the infectious period, the contact elicitation window, the contact definition, testing
and monitoring guidance, and per-contact date rules (symptoms possible from/through from the
incubation period, monitoring end, PEP deadlines, susceptible-contact exclusion). Writes a blank
`contact_tracking_<disease>.xlsx` to `/data/output` whose date columns are Excel formulas from
each contact's first and last exposure dates, plus a sheet of the rules and sources. No
identifiers pass through the chat. Decision support.

### `assess_measles_exposure` (CD-07, toolset `cd`)

| Argument | Default | |
|---|---|---|
| `first_exposure_date`, `last_exposure_date` | (one required) | |
| `case_rash_onset_date` | | Checks the exposure fell within 4 days before to 4 days after rash onset. |
| `exposure_continuous`, `last_case_onset_date` | | |
| `setting`, `immunity_group` | `other`, from the setting | `general` (1 dose), `school_college_travel` (2 doses), `health_care_personnel` (2 doses; birth year does not count). |
| `birth_year`, `age_months`, `documented_mmr_doses`, `positive_measles_igg`, `lab_confirmed_measles` | | Evidence of immunity. |
| `pregnant`, `severely_immunocompromised`, `weight_kg`, `received_immune_globulin` | | PEP choice and IG dose. |
| `as_of` | today | Whether each PEP window is still open. |

Returns the immunity assessment with reasoning, MMR (72 hours) and IG (6 days) deadlines with
the IG dose (IGIM 0.5 mL/kg, maximum 15 mL; IGIV 400 mg/kg), exclusion dates from the disease
rules (day 5 after first exposure through day 21 after last exposure), priority groups, the
symptom-monitoring end date, and DPH program notes (`content/cd/measles_exposure.yaml`, from the
DPH 2022 measles chapter and the 2013 ACIP statement). Decision support.

### `detect_aberrations` (CD-10, toolset `cd`)

| Argument | Default | |
|---|---|---|
| `counts` | (required) | At least 8 periods, oldest first. |
| `frequency` | `weekly` | `daily`, `weekly`, or `monthly`. |
| `period_labels` or `start_date` | | Labels for the periods. |
| `methods` | all | `ears_c1`, `ears_c2`, `ears_c3`, `cusum`, `farrington`. |
| `ears_min_sigma` | 0.5 when counts are sparse | Floor for the baseline SD. |
| `cusum_k`, `cusum_h`, `cusum_baseline_periods` | 0.5, 4, first half (≤ 52) | |
| `farrington_alpha` | 0.05 | |
| `report_last`, `chart` | 12, false | |

EARS C1/C2/C3 (7-period baseline; thresholds 3, 3, 2), a Poisson-standardized CUSUM (reset after
a signal), and the Farrington-type algorithm with the Noufaily (2013) improvements (up to five
years of history, ±3-week window, 10 seasonal levels, outbreak down-weighting, negative-binomial
threshold, at least 5 cases in the last 4 periods). Farrington needs at least one year of history
and is skipped for daily counts. Golden tests: R `surveillance` 1.26.1 (`earsC`, `algo.cusum`,
`farringtonFlexible`). Returns signals per method, thresholds for the most recent periods, an
interpretation, and the trade-offs between methods.

### `infection_control_for_congregate_settings` (CD-13, toolset `cd`)

| Argument | Default | |
|---|---|---|
| `pathogen` | (required) | Name or alias: "norovirus", "C. diff", "flu", "scabies", "MRSA", … |
| `setting` | all | `long_term_care`, `school`, `child_care`, `shelter`, `day_program`. |

Returns transmission-based precautions and their duration (CDC/HICPAC Appendix A), what each
precaution means and its PPE (105 CMR 300.020), cohorting and hand hygiene notes, the EPA lists
of registered disinfectants with claims against the pathogen (with links; products are matched by
EPA registration number), and DPH notes for the setting (for example, shelters and day programs
follow general-population guidance). Decision support.

### MCP prompt: `investigate_outbreak`

Arguments (all optional): `disease`, `setting`, `line_list_path`, `exposure_datetime`. Returns
one message that walks the model through the ten steps of a field investigation (CDC Field
Epidemiology Manual, chapter 3) adapted to a Massachusetts board of health — notify DPH
Epidemiology (617-983-6800), confirm the diagnosis and the outbreak (`detect_aberrations`), define
and count cases (`analyze_outbreak_line_list`), describe time, place, and person
(`generate_epi_curve`), start control measures (`calculate_isolation_and_exclusion`,
`infection_control_for_congregate_settings`, `plan_contact_tracing`, `assess_measles_exposure`),
test hypotheses (`analyze_two_by_two`), and communicate (`export_document`, MAVEN as DPH
directs) — with privacy rules for the whole investigation.

---

## Phase 4: Public data and evidence platform

<!-- Phase 4 tool sections go below this line, one `### \`tool_name\` (PLAN-ID, toolset \`x\`)` section per tool. -->

---

## Phase 5: Immunizations I

<!-- Phase 5 tool sections go below this line, one `### \`tool_name\` (PLAN-ID, toolset \`x\`)` section per tool. -->

---

## Phase 6: Immunization forecasting and surveillance feeds

<!-- Phase 6 tool sections go below this line, one `### \`tool_name\` (PLAN-ID, toolset \`x\`)` section per tool. -->
