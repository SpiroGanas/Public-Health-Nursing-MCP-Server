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

---

## Phase 4: Public data and evidence platform

<!-- Phase 4 tool sections go below this line, one `### \`tool_name\` (PLAN-ID, toolset \`x\`)` section per tool. -->

---

## Phase 5: Immunizations I

<!-- Phase 5 tool sections go below this line, one `### \`tool_name\` (PLAN-ID, toolset \`x\`)` section per tool. -->

---

## Phase 6: Immunization forecasting and surveillance feeds

<!-- Phase 6 tool sections go below this line, one `### \`tool_name\` (PLAN-ID, toolset \`x\`)` section per tool. -->
