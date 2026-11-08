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
| `vaccine_recommendation_basis` | Which body the immunization tools treat as the basis: `dph_then_professional_then_cdc` (default: DPH where it has a position, else AAP for children, ACOG for pregnancy, AAFP for adults, else CDC/ACIP), `cdc_acip`, `professional_societies`, or `show_all`. `default` clears it. Every answer still shows each body's position side by side. |
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

Phase 4 tools read public data through a live → cache → bundled-snapshot chain, so they work
offline. Each answer says which was used (`mode` on every source). Bundled snapshots (see
`snapshots/manifest.json` for URLs, checksums, and derivations): ACS 2020–2024 5-year
indicators for the U.S., Massachusetts, 14 counties, 351 cities and towns, and 2020 census
tracts; B16001 detailed languages for Massachusetts and its 2020 PUMAs; CDC/ATSDR SVI 2022 for
Massachusetts tracts; CDC PLACES 2025 for Massachusetts counties, places, tracts, and ZCTAs;
census geographies with tract-to-town and ZCTA-to-town crosswalks; and simplified map
boundaries. Optional keys (`PHN_CENSUS_API_KEY`, `PHN_NCBI_API_KEY`) are read from the
environment, never cached or cited; everything works without them.

### `build_community_health_profile` (EQ-01, toolset `equity`)

One-call profile of a city or town, county, or census tract.

| Argument | Default | |
|---|---|---|
| `area` | configured town | Town name ("Manchester-by-the-Sea"), "Hampden County", or an 11-digit tract GEOID. Misspellings get suggestions. |
| `level` | detected | `town`, `county`, or `tract`. |
| `include_chronic_disease` | `true` | Add CDC PLACES estimates (15 measures; all 40 are in `get_places_estimates`). |
| `include_svi` | `true` | Add the Social Vulnerability Index summary. |

Sections: population and age; race and ethnicity; nativity and language; income and poverty;
health insurance and disability; housing (renters, cost burden, crowding), vehicles, and
internet; education. Every figure names its ACS detailed table (B01001, C16001, C17002, …), the
5-year period, and the 90% margin of error, with Massachusetts and U.S. values beside it.
Derived percentages and their margins use the Census Bureau's approximation formulas
(*Understanding and Using ACS Data*, 2020, ch. 8); figures with a coefficient of variation above
30% are marked † and listed in a caveat. Live data come from `api.census.gov` (the newest
5-year vintage, found by probing the API's variable list for the next years); without a key the
API may answer with a "missing key" page, and the tool then uses the bundled snapshot and says
why. PLACES: the CDC place estimate when a city's census place matches the town, otherwise the
adult-population-weighted mean of the town's tract estimates (derived by this server; no
interval). SVI: the tracts that belong mainly to the town (largest share of their census
blocks); for the 37 small towns without one, the tracts that overlap the town, with a caveat.
Births and deaths are omitted: no machine-readable town-level vital statistics source was
reachable; the result says so. Read-only; open-world.

### `get_social_vulnerability_index` (EQ-02, toolset `equity`)

| Argument | Default | |
|---|---|---|
| `area` | configured town | Town, county, or tract GEOID. |
| `level` | detected | `town`, `county`, or `tract`. |
| `ranking` | `massachusetts` | Percentile ranks among Massachusetts tracts, or `national`. |
| `theme` | `overall` | Sort by `socioeconomic`, `household`, `minority_status`, or `housing_transportation`. |
| `top` | 10 | Most vulnerable tracts to list (1–200). |

Returns each tract's overall and four theme percentile ranks (0 least, 1 most vulnerable), the
number of flagged variables (90th percentile or above), counts by quartile, and the median rank;
for a single tract, its 16 SVI variables (percent estimates). Source: CDC/ATSDR SVI 2022 (ACS
2018–2022), bundled. Caveats: ranks are relative, cannot be compared across releases, and the two
rankings differ. Read-only; not open-world.

### `language_access_profile` (EQ-03, toolset `equity`)

| Argument | Default | |
|---|---|---|
| `area` | configured town | Town, county, or tract GEOID. |
| `level` | detected | |
| `eligible_population` | residents 5+ | Size of the population your program serves; group counts are scaled assuming the area's language mix. |

For each of the 12 C16001 language groups: speakers age 5+, people who speak English less than
"very well" (LEP) with the margin of error, and whether the group reaches the HHS LEP guidance
safe harbor for written translation (5% of the eligible population or 1,000 persons, whichever is
less): *meets even at the low end of the margin*, *meets*, *possibly* (within the margin), or
*no*, plus the guidance's fewer-than-50-persons rule. Also limited-English households (C16002),
detailed languages (B16001) for the PUMA(s) that contain the area, and Massachusetts's 15 most
common LEP languages (the list 45 CFR 92.11(b) points to). The rules come from
`content/equity/language_access.yaml` (pending review; the 2003 safe harbor entry is **low
confidence** because the HHS guidance could not be retrieved, and the tool says so in every
answer). Decision support, not legal advice. Read-only; open-world.

### `measure_health_disparities` (EQ-04, toolset `equity`)

| Argument | Default | |
|---|---|---|
| `groups` | | 2–50 groups, each `label`, `count`, `population`. Give `groups` or `path`. |
| `path`, `sheet` | | An aggregated CSV/XLSX in `/data`, one row per group (record-level files are refused). |
| `group_column`, `count_column`, `population_column` | detected | Columns in the file (count: count/cases/events/deaths; population: population/pop/denominator). |
| `reference` | `most_favorable` | `largest` or `named` (with `reference_label`). |
| `favorable_outcome` | `false` | `true` when higher is better (screening, vaccination). |
| `per` | 100,000 | Rate base. |
| `confidence` | 0.95 | 0.80–0.99. |
| `event_description` | | |

Methods: rates with exact Poisson intervals; rate ratios with the exact conditional (binomial)
interval, as R `poisson.test`, and the log-scale Wald interval; rate differences with the
Poisson variance; the index of disparity (Pearcy & Keppel, *Public Health Rep* 2002); the
population-weighted between-group variance (Harper et al., *Am J Epidemiol* 2008; NCI HD*Calc)
with a delta-method standard error; excess events and the population attributable fraction
(share of events that would not occur if every group had the reference rate) with an interval on
ln(1 − PAF). Golden tests: R (`poisson.test`, `binom.test`, closed forms; Monte Carlo check of
the variance SE). From a file, groups with counts from 1 to the suppression threshold − 1 are
left out of every measure so their counts cannot be recovered; identifier columns are refused.
Crude rates only: age-adjust first (`age_adjust_rates`) when groups differ in age. Disparities
between ACS survey estimates are not computed here (their sampling error is not Poisson); compare
those with the margins of error in `build_community_health_profile`. Read-only; not open-world.

### `get_places_estimates` (CHR-01, toolset `chronic`)

| Argument | Default | |
|---|---|---|
| `area` | | Town or city, "X County", tract GEOID, 5-digit ZIP/ZCTA, or a census place name. |
| `level` | detected | `county`, `town`, `place`, `tract`, or `zcta`. |
| `measures` | all 40 | Measure IDs or names ("DIABETES", "Obesity"). |
| `category` | all | "Health Outcomes", "Prevention", "Health Risk Behaviors", "Disability", "Health Status", "Health-Related Social Needs". |
| `age_adjusted` | `false` | Age-adjusted prevalence (counties and places only). |

CDC PLACES 2025 release (datasets on data.cdc.gov: county `swc5-untb`, place `eav7-hnsx`, tract
`cwsq-ngmh`, ZCTA `qnzd-25i4`), fetched live through the Socrata API with the bundled snapshot as
fallback. Each measure shows the estimate, its 95% confidence interval, the BRFSS data year
(2023 for most measures, 2022 for five), the release year, and Massachusetts and U.S. reference
values with "higher", "lower", or "similar" when the area's interval excludes the Massachusetts
value. Towns: the CDC place estimate when the city's census place has the town's boundary;
otherwise the adult-population-weighted mean of the tract estimates (no interval, derived by
this server). Massachusetts and U.S. values are adult-population-weighted means of county
estimates computed by this server, not official CDC state figures. Caveat in every answer:
model-based small-area estimates, not local survey results. Read-only; open-world.

### `create_choropleth_map` (EPI-12, toolset `epi`)

| Argument | Default | |
|---|---|---|
| `geography` | | `town`, `tract` (11-digit GEOID), `zcta` (5-digit), or `school_district` (DESE code or name). |
| `title` | | Map title. |
| `values` | | Inline `{area, value, count}` list. Give `values` or `path`. |
| `path`, `sheet` | | An aggregated CSV/XLSX in `/data`, one row per area. |
| `area_column`, `value_column`, `count_column` | detected | Columns in the file. |
| `value_is_count` | `false` | The mapped values are counts (small counts are suppressed). |
| `method` | `natural_breaks` | Or `quantile`. |
| `classes` | 5 | 3–7 (fewer when values are few). |
| `extent` | `data` | Zoom to the mapped areas, or `state`. |
| `subtitle`, `value_label`, `decimals`, `labels`, `source_note`, `file_name` | | Text and layout. |

Writes an SVG (with `<title>` and `<desc>`) and a PNG to `/data/output/maps/`. Classes use the
validated sequential blue ramp (`phn_docs::palette`); natural breaks are Fisher's exact optimal
partition (the Jenks criterion) and the goodness of variance fit is reported. Areas with a count
from 1 to the suppression threshold − 1 (or a suppression marker such as `*` or `<5` in the file)
are hatched, as are areas without data; neighboring areas outside the data are drawn in outline,
and town borders are overlaid on tract and ZCTA maps. Returns the file paths, alt text, and the
class table; per-area values are returned only for inline input. Files with direct identifiers,
repeated areas, or more than 5,000 rows are refused as record-level data. Boundaries: Census
cartographic boundary files (2024 towns and tracts, 2020 ZCTAs), simplified; school districts are
unions of member towns. Writes files; not open-world.

### `search_pubmed` (EBP-01, toolset `ebp`)

| Argument | Default | |
|---|---|---|
| `population`, `intervention`, `outcome` (and `comparison`) | | PICO elements: `phrase`, optional `synonyms` (title/abstract; `*` truncates), optional `mesh` headings. |
| `include_comparison` | `false` | Add the comparison to the query (usually left out). |
| `query` | | A ready-made PubMed query, alone or with PICO elements. |
| `filters` | | `systematic_review`, `meta_analysis`, `randomized_controlled_trial`, `guideline`, `humans` (study types are ORed). |
| `from_year`, `to_year`, `languages` | | |
| `max_results` | 10 | 1–50. |
| `sort` | `relevance` | Or `newest`. |

Each element becomes `("MeSH"[MeSH Terms] OR "phrase"[tiab] OR "synonym"[tiab])`: given MeSH
headings are checked against the MeSH database (unknown ones are dropped and reported), and
PubMed's automatic term mapping adds the heading for the whole phrase. Elements are ANDed with the
filters. Returns the exact query (and PubMed's translation and a link to rerun it), the total
count, and for each record the PMID, title, journal, year, first three authors, an abstract
excerpt, DOI, publication types, and link. NCBI E-utilities (`esearch`, `efetch`) with
`tool=public-health-nursing-mcp-server` and no email; requests are spaced to stay under NCBI's
limit (3 per second, 10 with `PHN_NCBI_API_KEY`) and cached for a day, so a recent search also
works offline. Read-only; open-world.

### `search_cdc_guidance` (EBP-02, toolset `ebp`)

| Argument | Default | |
|---|---|---|
| `topic` | | E.g. "measles", "Candida auris". |
| `series` | all | `mmwr`, `han`, `stacks`. |
| `since` | | Only items on or after this date (YYYY-MM-DD). |
| `max_results` | 10 | Per series, 1–30. |

Newest first, with title, date, series, link, and CDC's summary. Methods (CDC reorganized its
site in 2024–2025 and retired several feeds; checked November 2026): **MMWR** — the CDC Content
Syndication API v2 (`tools.cdc.gov/api/v2/resources/media`, title search, MMWR pages only) plus a
PubMed search of the MMWR journal, which also matches abstracts. **HAN** — the HAN archive's JSON
module, then the next notice pages in sequence until one is missing (so new notices are found
before the archive is refreshed), plus the retired HAN feed for 2023 and earlier; matched by
words in the title and summary. **CDC Stacks** — its search refuses automated requests and its
OAI-PMH interface has no topic search, so the tool returns a Stacks search link instead of
results. Each series reports how it was searched and whether it could be reached. Decision
support; follow DPH where state requirements differ. Read-only; open-world.

---

## Phase 5: Immunizations I

Immunization content is drafted from documents retrieved 2026-11-07 (DPH, CDC/ACIP, AAP, ACOG,
AAFP, the Northeast Public Health Collaborative, Immunize.org) and is **pending review** by the
subject matter expert; every answer built on it says so. Since 2025 federal CDC/ACIP guidance has
differed from DPH and the professional societies, so the tools show each body's position with its
date and order them by the `vaccine_recommendation_basis` setting (see `configure_jurisdiction`).
Clinical answers carry the decision-support notice.

### `get_immunization_schedule` (VAX-01, toolset `vax`)

| Argument | Default | |
|---|---|---|
| `vaccine` | all vaccines | Vaccine or disease ("MMR", "hep B", "Shingrix"); fuzzy matched. |
| `population` | `child` | `child` (birth–18) or `adult`. |
| `include_minimums` | true when a vaccine is named | CDC minimum ages and intervals (General Best Practice Guidelines, Table 3-2). |
| `include_contraindications` | true when a vaccine is named | CDC Table 4-1 contraindications and precautions. |

Returns the routine schedule with each body's position (basis first), the schedule editions in use
with their dates, and general rules (4-day grace period, 28-day spacing of live vaccines). It does
not forecast a person's doses; catch-up forecasting is VAX-03 (Phase 6).

### `compare_vaccine_recommendations` (VAX-02, toolset `vax`)

| Argument | Default | |
|---|---|---|
| `query` | list every topic | Vaccine or question ("hepatitis B birth dose", "COVID pregnancy", "MMRV"). |
| `population` | | `infants`, `children`, `adolescents`, `adults`, or `pregnancy`. |
| `basis` | saved setting | Override for this answer. |

For each topic: every body's position with date, source, and confidence (`high` = from the body's
own document; `medium` = described by another document), whether they differ and how, and which
bodies have no position on file. 17 topics, including the childhood and adult schedules, the
hepatitis B birth dose, MMRV under 4, COVID-19 (children, adults, pregnancy), influenza, RSV,
hepatitis A, rotavirus, MenACWY, Tdap in pregnancy, and thimerosal.

### `check_ma_school_immunization_requirements` (VAX-04, toolset `vax`)

| Argument | Default | |
|---|---|---|
| `grade` | required (one student) | `PK` (child care/preschool, K0/K1), `K`, `1`–`12`, `college`, or `ungraded`. |
| `date_of_birth` | required (one student) | YYYY-MM-DD; used for the calculation and not repeated. |
| `as_of` | today | Evaluation date, e.g. the first day of school. |
| `doses` | | Dose dates by vaccine key: `dtap`, `dt`, `dtap_ipv`, `dtap_hepb_ipv` (Pediarix), `dtap_ipv_hib` (Pentacel), `dtap_ipv_hib_hepb` (Vaxelis), `polio`, `hepb`, `heplisav_b`, `hib`, `mmr`, `mmrv`, `varicella`, `tdap`, `td`, `menacwy`, `menabcwy`. MenB never counts. |
| `lab_immunity` | | Laboratory evidence on file: `hepb`, `measles`, `mumps`, `rubella`, `varicella`. |
| `varicella_history` | false | Reliable history of chickenpox. |
| `exemptions` | | `[{vaccine, kind}]`, kind `medical` or `religious`; vaccine `all` covers every requirement. |
| `college` | | `health_science`, `full_time`, `born_in_us` (birth-year rules). |
| `residential_combined_school` | false | Residential student in a school combining preschool–8 with 9–12. |
| `apply_grace_period` | true | CDC's 4-day grace period (DPH does not say whether it applies; see caveats). |
| `roster_path`, `sheet` | | Batch mode: a CSV/XLSX roster in `/data`. |

Rules come from DPH's 2026–2027 requirements document and 105 CMR 220.000. Each dose is first
validated against CDC minimum ages and intervals (grace period, 28 days between different live
vaccines), then each requirement's options are checked; proof of immunity, then exemptions, are
applied in DPH's "count each child once" order. Each requirement is `met`, `not_met`,
`not_yet_due`, `exempt_medical`, `exempt_religious`, `not_applicable`, or `needs_review` (cases the
rules cannot decide, such as a 3-dose Hib series whose product is unknown), with the reason, the
doses that do not count and why, and the earliest date a missing dose would count (minimum age
and interval, also from an invalid dose).

**Batch mode** needs columns `Grade` and `Date of Birth` and one column per vaccine with dose
dates separated by semicolons; optional `Exemptions` (`mmr:medical; all:religious`),
`Varicella History`, `Lab Immunity`, and college columns. Per-student results (with the roster's
name and ID columns) go to `/data/output/school_immunization_check_<date>.xlsx`; the answer has
only counts by grade, requirement, and status, with small-cell suppression. Rows that cannot be
evaluated are listed in the output file, not the answer.

### `analyze_vaccination_coverage` (VAX-05, toolset `vax`)

| Argument | Default | |
|---|---|---|
| `town` / `county` / `school` | configured town, else statewide | Scope. |
| `statewide` | false | Ignore the configured town. |
| `grade` | `K` | `K`, `7`, or `K3` (kindergarten 2023–2026 combined, which releases more small schools). |
| `measure` | `mmr` | `mmr`, `dtap`, `polio`, `hepb`, `var`, `tdap`, `men`, `series`, `exemption`, `unimmunized`, `no_record`, `noncompliance`, or `influenza` (Massachusetts vs U.S. only). |
| `threshold_pct` | the measure's threshold | Flag schools below this percent (95 for MMR). |
| `limit` | 25 | Schools listed (max 200). |
| `local_path`, `sheet` | | Your own school-level counts: columns `school`, `enrolled`, and a count named after the measure (or `up_to_date`). |

Uses DPH's school immunization survey (2025–2026 kindergarten and grade 7, and the 2023–2026
combined kindergarten file; bundled snapshot) and CDC NIS-Child, NIS-Teen, FluVaxView, and
SchoolVaxView estimates for Massachusetts and the U.S. (bundled snapshot from data.cdc.gov).
Coverage measures rank lowest first; exemption and other burden measures highest first. Schools
DPH did not release (fewer than 30 students or unresolved discrepancies) and those that did not
respond are counted, not ranked. Town figures are unweighted summaries of school rates because
DPH publishes rates without enrollment. Herd-immunity thresholds cite Fine, Eames, and Heymann
(2011). In a local file, rates are shown only for schools with 30 or more students (DPH's rule);
student-level files are refused.

### `vaccine_temperature_excursion_helper` (VAX-06, toolset `vax`)

| Argument | Default | |
|---|---|---|
| `unit_type` | required | `refrigerator`, `freezer`, or `ultra_cold`. |
| `min_temp`, `max_temp` | | Lowest and highest readings during the event. |
| `temp_unit` | `C` | `C` or `F`. |
| `minutes_out_of_range` | | Cumulative minutes out of range in 24 hours. |
| `state_supplied` | true | State-supplied (MA Vaccine Program) vaccine in the unit. |
| `open_multidose_vials` | false | |
| `discovered_at`, `vaccines` | | For the worksheet (vaccine name, manufacturer, lot, expiration, doses, funding; no patient data). |
| `worksheet` | none | `xlsx` or `docx` excursion worksheet in `/data/output`. |

Classifies the readings against the normal range and the MA Vaccine Program's reporting
thresholds (`in_range`, `out_of_range_below_reporting_threshold`, `reportable_cumulative`,
`reportable_immediate`, `undetermined`) and lists the CDC Storage and Handling Toolkit steps,
DPH's steps for state-supplied vaccine, and contacts. It never decides viability: only the
manufacturer can, and for state-supplied vaccine the Vaccine Program then gives instructions.

### `lookup_vaccine_product` (VAX-07, toolset `vax`)

| Argument | Default | |
|---|---|---|
| `query` | required | Name, trade name, CVX ("03", "CVX 208"), MVX ("PMC"), or NDC (10 or 11 digits, with or without dashes). Fuzzy matched. |
| `include_inactive` | false | Include inactive codes and NDCs that are no longer marketed. |
| `limit` | 5 | Matches (max 25). |
| `include_label` | false | Also search DailyMed live for the current package insert. |

Returns the CVX description and status, trade names and MVX manufacturers, marketed NDCs (unit of
sale and unit of use), licensed ages and storage where bundled content has them, the current
Vaccine Information Statement edition, and the Immunize.org VIS translations page. The CVX table
is refreshed live from CDC when reachable (cached a week) and otherwise comes from the bundled
snapshot, as do MVX, trade names, and NDCs.

### `plan_vaccination_clinic` (VAX-08, toolset `vax`)

| Argument | Default | |
|---|---|---|
| `expected_turnout`, `clinic_hours` | required | |
| `doses_per_person` | 1 | e.g. 2 for flu plus COVID-19. |
| `vaccines` | | One reconciliation row each. |
| `population` | `mixed` | `children`, `adolescents`, `adults`, or `mixed` (pediatric autoinjector strengths). |
| `doses_per_vial` | single-dose | Vials to bring. |
| `doses_per_vaccinator_per_hour`, `registrations_per_hour`, `screenings_per_hour`, `observation_minutes`, `waste_factor_pct`, `observation_seat_margin_pct` | tool defaults | Overrides. |
| `epinephrine_weight_kg` | | Weight-based epinephrine dose (0.01 mg/kg, maximum 0.5 mg) and the matching age/weight band. |
| `reconciliation_sheet` | true | Dose reconciliation XLSX in `/data/output`. |

Staffing is arrivals (or doses) per hour divided by each station's rate; observation seats are
arrivals per hour × observation minutes ÷ 60 plus a margin (25% by default). Every assumption is listed with its basis:
`source` (cited: 15-minute observation from CDC, predrawing limits from the CDC toolkit, at least
three epinephrine doses from Immunize.org), `tool_default` (planning figures with no published
source, to be replaced with the agency's experience), or `your_override`. The anaphylaxis kit,
protocol, and epinephrine and diphenhydramine tables are transcribed from DPH Standing Orders
2026-03 and 2026-04 and Immunize.org "Medical Management of Vaccine Reactions" (P3082a, P3082).

### `vaccine_confidence_conversation_guide` (VAX-09, toolset `vax`)

| Argument | Default | |
|---|---|---|
| `concern` | required | In the person's words ("worried about autism", "too many shots", "HPV at 11"). |
| `target_grade` | 8 | Target reading grade (3–16). |
| `language` | English | Other languages get links to authoritative translations, never machine translation. |
| `max_facts` | 3 | Talking points (1–6). |

A motivational-interviewing sequence: an open question, a reflection, asking permission, one to
three facts each cited to CDC, AAP, Immunize.org, or DPH with its date, and a next step. Where
federal pages now differ from the professional societies (for example CDC's autism page), the
guide says so. The Flesch–Kincaid grade of the script is computed with the standard formula
(0.39 × words per sentence + 11.8 × syllables per word − 15.59) and a vowel-group syllable count.
Topics: safety, ingredients, autism, too many too soon, the changed federal schedule, HPV,
COVID-19, influenza, and pregnancy.

### `lookup_medication_information` (CORE-07, toolset `core`)

| Argument | Default | |
|---|---|---|
| `query` | required | Brand or generic name. |
| `include` | all | Any of `rxnorm`, `label`, `recalls`, `shortages`, `dailymed`. |
| `limit` | 3 | Results per section (max 10). |

Live lookups (cached a day) in RxNorm (RxNav: best concept, ingredients, brand names), openFDA
(label highlights with effective date, set ID, and version; enforcement reports; drug shortages),
and DailyMed (package inserts with version and publication date). openFDA searches use the RxNorm
ingredient name when there is one. An optional free key in `PHN_OPENFDA_API_KEY` raises openFDA's
rate limit and is never shown. There is no offline copy: offline or unreachable sections are
reported as not checked. Reference only; it gives no patient-specific dosing.

---

## Phase 6: Immunization forecasting and surveillance feeds

### `forecast_immunizations_due` (VAX-03, toolset `vax`)

Evaluates a dose history and forecasts the doses due with CDC's Clinical Decision Support for
Immunization (CDSi): the *Logic Specification for ACIP Recommendations* v4.6 (December 2024)
implemented in the `phn-cdsi` crate over CDC's *Supporting Data* v4.65 (August 2026), which is
embedded byte for byte with SHA-256 checksums (`crates/phn-cdsi/data/manifest.json`).

| Argument | |
|---|---|
| `date_of_birth` | YYYY-MM-DD (one-person mode). Used for the calculation only; the answer shows ages. |
| `sex` | `female`, `male`, or `unknown` (default). The HPV series differ by sex. |
| `assessment_date` | YYYY-MM-DD (default today). |
| `doses` | `[{vaccine, date}]`: CVX code, vaccine type (`DTaP-HepB-IPV`), or trade name (`Pediarix`), resolved to CVX from the supporting data and the names in CDC's test cases. Ambiguous or unknown names are a tool error with suggestions. |
| `conditions` | `[{condition, date?}]`: CDSi observation codes or titles (`007` / `Pregnant`, `Anatomical or functional asplenia`, `Health care personnel`). They select risk series, contraindications, and evidence of immunity. |
| `born_in_us` | Needed for birth-date evidence of immunity that applies only to U.S. births (varicella before 1980). |
| `vaccine_groups` | Limit to groups (`DTaP`, `MMR`, `HPV`, `pneumococcal`, `flu`, ...). |
| `file`, `sheet` | File mode: a CSV/XLSX in `/data`, one row per dose. |
| `id_column`, `dob_column`, `date_column`, `vaccine_column`, `sex_column` | File-mode column names (detected when omitted). |

**One person.** Returns a status per vaccine group (overdue, due now, can give now, due later,
complete, immune, contraindicated, aged out, not recommended now) with the next dose number and
the earliest, recommended, and past-due dates; each dose's evaluation (valid, not valid,
extraneous) with reasons (too young, too soon, grace period, live virus conflict, not a
preferable or allowable vaccine); contraindications and evidence of immunity found; and a
projected catch-up schedule. The projection re-runs the engine as if each remaining dose were
given on its earliest date, so doses that become unnecessary with age drop out; it stops at
routine doses more than two years away. When CDSi produces more than one forecast for a vaccine
group (one per series group, such as a risk series and the standard series, or the childhood and
age-50+ pneumococcal series), all are listed; forecasts first due more than a year away are
summarized on one line.

**File mode.** Each person is evaluated and forecast; the per-person forecasts and dose
evaluations (with person IDs) are written to `/data/output/<file>_forecast_<date>.csv` and
`<file>_dose_evaluations_<date>.csv`. The response holds only the number of people by vaccine
group and status, with small-cell suppression. Rows with a missing ID, date of birth, date, or
vaccine are skipped and counted; unrecognized vaccines are counted and listed in the output file.

**Validation.** `crates/phn-cdsi/tests/cdsi_test_cases.rs` runs all 1,401 CDC test cases (healthy
childhood and adult v4.46; underlying conditions v4.6). All 1,064 healthy cases pass; 36 older
underlying-condition cases are documented exceptions with reasons
([`docs/cdsi-test-results.md`](cdsi-test-results.md)). The test fails on any new failure and on
any exception that starts passing.

Every answer is decision support: verify against the MIIS record before acting. CDSi encodes
CDC/ACIP recommendations only; the answer says that DPH and AAP/ACOG positions may differ since
2025 and points to `compare_vaccine_recommendations`. Massachusetts school requirements are a
separate tool (`check_ma_school_immunization_requirements`). Seasonal series (influenza,
COVID-19, RSV) follow the season dates in the embedded supporting data; after that season ends
they show "not recommended now" until CDC publishes new supporting data.

### `get_notifiable_disease_trends` (CD-08, toolset `cd`)

Weekly National Notifiable Diseases Surveillance System (NNDSS) counts from CDC's *NNDSS Weekly
Data* (data.cdc.gov `x9gk-5huc`, MMWR years 2022 onward) for Massachusetts, New England, and
U.S. residents, compared with the same point in the five previous MMWR years.

| Argument | |
|---|---|
| `condition` | NNDSS condition name or part of it (`pertussis`, `measles`, `hepatitis A`, `whooping cough`). Every NNDSS row that matches is shown (up to 12; for example measles indigenous and imported). Omit for an overview. |
| `areas` | Reporting areas (default Massachusetts, New England, United States): a state, an NNDSS region, or `United States`. At most 6. |
| `year`, `week` | MMWR year and week (default the latest published). |
| `limit` | Overview rows (default 25, at most 150). |

**Method** (also returned in `data.method`):

- *Year to date (YTD)*: the cumulative count through MMWR week *w* as published that week (NNDSS
  column m3; `-` counts as 0). For 2021, the year before the dataset starts, the previous-year
  column (m4) of the 2022 rows is used. Week 53 is read as week 52 in 52-week years.
- *Last 4 weeks*: YTD(*w*) − YTD(*w* − 4), the cases added to the cumulative total in the four
  weeks ending week *w* (net of revisions, so it can be negative).
- *Baseline*: the same quantity in each of the five previous MMWR years: mean, minimum, maximum,
  and ratio (current / mean). It needs at least 3 of the 5 years; NNDSS rows that CDC renamed or
  redefined have shorter histories and are not combined with similarly named rows. "Above" or
  "below the 5-year range" means outside the baseline minimum–maximum.
- Baseline years use counts as published at the same week, so they carry reporting delays like
  the current year's. CDC's revised previous-year figure (m4) is shown alongside.
- *Year-end totals*: YTD at the last week of each previous year (provisional, as published at
  year end). Finalized annual tables are on CDC Stacks
  (<https://www.cdc.gov/nndss/infectious-disease/weekly-and-annual-disease-data-tables.html>);
  CDC WONDER's annual query could not be reached from the development environment.

**Condition mode** returns, per area: this week's count, YTD with the baseline, the last 4 weeks
with the baseline, provisional year-end totals for the previous five years, and the first area's
weekly counts for the last 8 weeks. **Overview mode** (no condition) lists every condition with
cases this year or in the baseline for the first area, those above their 5-year range first.

Conditions not in the weekly tables (for example Lyme disease) are a tool error that points to
CDC's annual tables and `lookup_reportable_condition`. Data come through the live → cache →
snapshot chain; the bundled snapshot (`cdc-nndss-weekly`) holds the latest week only (that week
and the 7 before it, plus weeks 52–53) for Massachusetts, New England, and U.S. residents, and
omits series with no reported cases. Counts are provisional; DPH (MAVEN) counts can differ.

### `get_respiratory_and_wastewater_surveillance` (CD-09, toolset `cd`)

A weekly briefing on COVID-19, influenza, and RSV in Massachusetts from CDC data on
data.cdc.gov.

| Argument | |
|---|---|
| `pathogens` | `COVID-19`, `influenza`, `RSV` (default all three). |
| `weeks` | Weeks of history per measure (default 4, 2–12). |

| Measure | Source (dataset) | Geography |
|---|---|---|
| Hospital admissions per 100,000 and CDC level | NHSN Weekly Hospital Respiratory Admission Levels and Rates (`vdzy-6i9v`) | Massachusetts, with the U.S. for context |
| Percent of emergency department visits and CDC trend | NSSP ED Visit Trajectories (`rdmq-nq56`), state level | Massachusetts, with the U.S. |
| Test positivity | NREVSS SARS-CoV-2 (`gvsb-yw6g`) and RSV (`3cxc-4k8q`) | HHS Region 1 (New England), with national |
| Influenza test positivity | Percent of Tests Positive for Viral Respiratory Pathogens (`seuz-s2cv`) | National |
| Acute respiratory illness activity level | Level of ARI Activity by State (`f3zz-zga5`) | Massachusetts |
| Wastewater viral activity level (WVAL) | CDC Wastewater Viral Activity Level (`atcp-73re`) | Each Massachusetts sampling site |
| Deer Island SARS-CoV-2 (final) | MWRA Biobot data PDF (bundled; samples through 2026-07-27) | MWRA Southern and Northern systems |

For each measure the latest week is compared with the week before: *up* when at least 10%
higher, *down* when at least 10% lower, otherwise *stable* (this server's rule, stated in
`data.method`). CDC's own categories are reported as given: NHSN admission levels, NSSP trends,
the ARI level, and WVAL categories. The wastewater summary counts sites that reported in the
latest week, by category, with the median WVAL across them (CDC computes state values as the
median across treatment plants). A data-freshness table gives each source's latest week and its
lag in days. DPH's respiratory illness dashboard
(<https://www.mass.gov/info-details/respiratory-illness-reporting>) is linked as the
Massachusetts source of record and is not scraped.

MWRA's Biobot testing at Deer Island ended with samples collected through 2026-07-27 (MWRA
reports DPH is replacing it with its own program); the final values come from a bundled snapshot
read from MWRA's data PDF. If a source cannot be reached, the briefing uses the bundled
snapshots (`cdc-respiratory-ma`, `cdc-nwss-ma`; the last 13 weeks) and says so; a source with no
data at all is left out with a caveat.

## Phase 7: School health

<!-- Phase 7 tools go below this line. -->
<!-- End of Phase 7 tools. -->

## Phase 8: Maternal, child, and family health

<!-- Phase 8 tools go below this line. -->
<!-- End of Phase 8 tools. -->

## Phase 9: PHN practice toolkit and case management

<!-- Phase 9 tools go below this line. -->
<!-- End of Phase 9 tools. -->

## Phase 10: Healthy aging and mental health screening

<!-- Phase 10 tools go below this line. -->
<!-- End of Phase 10 tools. -->

## Phase 11: Substance use, crisis resources, and health communication

<!-- Phase 11 tools go below this line. -->

### `check_readability` (COMM-01, toolset `comms`)

| Argument | Default | |
|---|---|---|
| `text` or `path` | required | The draft as plain text or Markdown, or a DOCX, HTML, PDF, TXT, or Markdown file in `/data`. |
| `target_grade` | 8 | Reading grade to aim for (a tool default; set your agency's standard). |
| `cci_answers` | | Your CDC Clear Communication Index answers: `"1"`–`"20"` → `yes`/`no` (`na` for 19 and 20), plus `"C"` and `"D"` → `yes`/`no` for whether Parts C (numbers) and D (risk) apply. |
| `max_flags` | 10 | Most flags listed per kind (max 50). |

Counts words, sentences, and syllables (headings excluded; each list item is a sentence;
hyphenated words count once) and reports Flesch Reading Ease, Flesch-Kincaid, SMOG (formula and
CDC's conversion table), and Gunning Fog, with the median grade against the target and Fry's
per-100-word averages. Flags sentences over 20 words (CDC plain language checklist; ODPHP Health
Literacy Online), jargon from CDC's *Everyday Words for Public Health Communication* (124 terms,
with CDC's alternatives and before/after examples), possible passive voice, acronyms not spelled
out at first use, paragraphs over 5 sentences, lists over 7 items, negative instructions, and
numbers. The Clear Communication Index is scored from the nurse's answers following CDC's score
sheet (90 or higher passes); until every applicable item is answered, the tool shows its own
suggestions for the items it can judge (lists, headings, jargon, numbers). Excerpts quoted from a
file pass through the identifier redactor, and words that could be names are never listed.
Golden tests: CDC *Simply Put*'s SMOG table and hepatitis A Fry example, and the score sheet's
arithmetic (`crates/phn-tools/tests/readability_golden.rs`).
<!-- End of Phase 11 tools. -->

## Phase 12: Emergency preparedness I: situational awareness and planning

<!-- Phase 12 tools go below this line. -->
<!-- End of Phase 12 tools. -->
