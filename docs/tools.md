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

**Dates.** Date arguments are written YYYY-MM-DD. Dates in the nurse's files, and dates given to
the school, immunization, and growth tools, may also be written month first (3/7/2024 or 3/7/24)
or as an Excel date. A two-digit year is read in the current century unless that would be more
than a year in the future, in which case it is the previous century (3/7/24 is 2024; 5/2/50 is
1950). A year before 1900 or after 2100 makes a cell in a file unreadable, and the communicable
disease, school, immunization, and growth tools refuse such a date as an argument. (In an
outbreak line list, a cell holding a date and a time, such as `5/3/26 14:30`, reads a two-digit
year as 1969–2068.)

**Sources and the cache.** The retrieval date cited for a source is the local calendar date (the
container runs on Eastern time). Copies of public data saved in `/cache` are deleted after 90
days; a cache that cannot be read is skipped, so the live source or the bundled snapshot is used
instead.

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
that happens. In particular, when `PHN_PROFILE` or `PHN_TOOLSETS` is set there, a saved profile or
toolset list has no effect; a call to a tool outside the enabled toolsets then returns an error
saying that the client configuration must be edited. A `suppression_threshold` outside 2–30 in the
settings file (for example after a hand edit) is never used: the default applies, and the result
and the server's startup log say so. HMCC regions are derived from the 2005 DPH preparedness-region layer (4A and 4B
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
It does not guess: a query that fits more than one condition ("hepatitis"), is too general
("fever", "stomach flu"), or is very short is answered as not matched, with the candidates
listed. An exact name, alias, or laboratory term always matches.

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
emails, addresses, and dates too). Headings and titles are redacted like the body. A document
that holds a table (a Word table, or tab-, pipe-, or semicolon-separated text) with a
client-identifier column, or several names next to dates or street addresses, is treated as a
client roster and redacted strictly. In sample rows, ZIP codes become three digits plus `xx`
(`000xx` for the low-population prefixes), a birth year that shows an age of 90 or more becomes
`90+`, and numbers of six or more digits in columns that were not recognized become
`[REDACTED: number]`. Column headers that are blank or look like data (a file with no header row)
are shown as `column_N`, and a sheet named after a person as `sheet_N` (use that name to select
it). There is no way to turn redaction off.

### `search_agency_policies` (CORE-05, toolset `core`)

| Argument | Default | |
|---|---|---|
| `query` | required | Words, "quoted phrases", AND/OR/NOT. |
| `folder` | `/data/policies` | |
| `max_results` | 5 | 1–20. |

Indexes PDF (by page), DOCX (by heading), Markdown (by heading), text, and HTML with `tantivy`
in `/cache/index`, rebuilding automatically when files change. Headings in passages and
citations are redacted like the text. A file that holds client identifiers (a date of birth, a
record or health plan number) is redacted strictly, headings included; an SSN alone does not
trigger this, because policies often show example numbers. Returns passages with query
terms in bold and citations such as `Exposure Control Plan.docx, § Reporting` or
`TB policy.pdf, p. 4`.

### `export_document` (CORE-06, toolset `core`)

| Argument | Default | |
|---|---|---|
| `title`, `subtitle` | | Title is also the document's title property. |
| `content_markdown` | required | Headings, paragraphs, bold/italic, lists (nested items stay under their parent item), links, pipe tables (`Table: caption` line before a table), images `![alt](path)`, `\pagebreak` or `<!-- pagebreak -->`. A `---` rule does not start a new page. Indented or fenced text is kept as plain paragraphs. |
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

In XLSX output, repeated column names get a number ("n (2)"), sheet names are cut to 28
characters, and only plain numbers are stored as numbers (ZIP codes with leading zeros, lists
such as "1,2", and long identifiers stay text). In CSV output, a cell that starts with `=`, `+`,
`-`, or `@` and is not a number gets a leading apostrophe, so a spreadsheet does not run it as a
formula.

**Privacy:** merged documents are written to `/data/output`; the response contains only counts,
field names, and generic file names. For a mail merge, the accessibility check runs on the
template, so no merged text comes back in the check's messages.

### `apply_small_cell_suppression` (EPI-06, toolset `epi`)

| Argument | Default | |
|---|---|---|
| `table` or `path` | | Inline `{columns, rows}` or a CSV/XLSX of counts. |
| `sheet` | | Sheet name for an XLSX file. |
| `label_columns` | detected | Leading text columns. |
| `threshold` | configured (5) | |
| `suppress_zeros` | false | |
| `complementary` | true | |
| `last_column_is_total`, `last_row_is_total` | detected from "Total" | |
| `marker` | `*` | Same marker for primary and complementary cells. |
| `write_output` | true for files | Writes `<name>_suppressed.csv` with a footnote. |

Complementary suppression hides additional cells until no row or column contains exactly one
hidden cell, preferring the smallest non-zero interior cells and touching totals only as a last
resort. Tables with direct identifiers are refused as record-level data. A leading numeric column
named like a label (Grade, Age, Year, Week, Month, ZIP, Code, ID, District, Tract, ...) is a label
column, not a count; so is a leading numeric column that is not part of the row sum when the last
column is a total. Label cells that look like free text (over 80 characters) or contain names,
phone numbers, or addresses are refused, because labels are returned as they are. Error messages
name the row and column of a bad cell but never quote it.

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
show values. A numeric column with fewer values than the suppression threshold shows no
statistics. In age columns, ages over 89 are counted as 90 before any statistic is computed. The
smallest and largest values are also hidden when almost every value differs, as ID numbers do.
"(missing)" is listed with its count, outside the suppression of the other categories, because
the number missing is reported anyway.

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
quasi-identifiers is shared by fewer than *k* records (*k* = the suppression threshold). Safe
Harbor also removes full dates and ZIP codes found inside text cells (a bare year stays; a compact
date such as 20240311 becomes its year). Ages in months, weeks, or days are not top-coded. A
limited data set drops latitude, longitude, and geocode columns, which locate a home as precisely
as a street address.
Writes the data and a markdown report to `/data/output`; only the report is returned.

### `check_document_accessibility` (COMM-05, toolset `comms`)

| Argument | |
|---|---|
| `path` | DOCX, HTML, or PDF in `/data`. |

Checks, with the WCAG 2.1 success criterion for each: title (2.4.2), language (3.1.1), alt text
(1.1.1), and scanned pages for every format; headings present (2.4.6) and in order (1.3.1), bold
paragraphs posing as headings, table header rows and merged cells (1.3.1), color contrast
(1.4.3), link text (2.4.4), empty paragraphs and typed bullets, and form labels (HTML, 3.3.2) for
DOCX and HTML; tagging, title display, figure alt text, and embedded fonts for PDF. For a PDF,
headings, table header cells, contrast, and link text are listed as manual checks (check the
source document, or use Acrobat's checker). Text quoted in the report (title, headings, first
table rows) has possible identifiers replaced with `[REDACTED: kind]`, and the privacy block
says when that happened. Lists failures with how to fix them in Word or
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
threshold. Caveats say when a group's age groups do not run from age 0 to an open-ended oldest
group (the rates then describe only those ages and are not comparable with published all-ages
rates) and when groups use different age groups. A comparison with a group whose rate is zero is
shown as "not estimable". Golden tests: R `epitools::ageadjust.direct`.

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
(a confounding screen). The Breslow–Day degrees of freedom count only strata with both margins
filled. Cells larger than 1,000,000,000 are refused as typing errors; when a table has more than
1,000,000 possible arrangements, the exact methods (Fisher, mid-p, conditional odds ratio) are
skipped with a note, and the chi-square tests and Woolf and Katz intervals are used. Golden tests:
R `fisher.test`, `chisq.test`, `mantelhaen.test`,
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
| `periods` | | In order: `label`, `count`, optional `population`. Labels such as "2024", "2024-03", "03/2024", "Mar 2024", "2024 Q2", "2024-W05", or a date place a seasonal peak in the calendar; other labels give the peak as a position counted from the first period. |
| `time_unit` | `year` | `quarter`, `month`, `week`. |
| `model` | `auto` | Poisson, switching to negative binomial when Pearson dispersion > 1.5. |
| `seasonal` | true for months and weeks with at least two years (24 months, 104 weeks) | One annual harmonic, tested by likelihood ratio. Turned off by default for shorter series (with a caveat); `true` is refused with fewer than 18 months, 78 weeks, or 6 quarters. |
| `max_joinpoints` | 3 | Limited to ⌊(n − 2)/5⌋ as in NCI Joinpoint guidance. |
| `interruptions` | | Labels of the first period after a known change (interrupted time series). |
| `per`, `confidence`, `event_description`, `chart` (true) | | |

Reports the percent change per year from one log-linear trend; the BIC-selected joinpoint model
(segment APCs and the AAPC; intervals conditional on joinpoint locations); level and slope changes
at interruptions; seasonality; Mann–Kendall τ; and Durbin–Watson autocorrelation warnings. Saves a
line chart of observed and modeled values. A series whose counts are all zero is refused. A model
that does not converge is reported in the caveats, and the Durbin–Watson statistic is left out
when the model fits every period exactly. Golden tests: R `glm`, `MASS::glm.nb`,
`trend::mk.test`, `trend::sens.slope`.

### `run_statistical_test` (EPI-10, toolset `epi`)

Give one kind of data: `groups` (name + values), `paired` (`before`, `after`), `table`
(`row_labels`, `column_labels`, `counts`), `x` and `y`, or a local file: `path` (+ `sheet`) with
`outcome` and `group`, `paired_with`, or `predictors` (regression; `outcome_event` for logistic).
`test` defaults to `auto`; `confidence` 0.95. Optional labels: `question` (the question in
words, repeated in the answer) and `x_name` and `y_name` (names for the two measurements).

Automatic choice: two groups → Welch t (or Mann–Whitney if not normal: Shapiro–Wilk p < 0.05
with n < 30); 3+ groups → ANOVA, Welch ANOVA (unequal variances by the median-centered Levene
(Brown–Forsythe) test), or
Kruskal–Wallis; paired → paired t or Wilcoxon signed-rank; counts → chi-square or Fisher's exact
(2×2 with an expected count < 5); two measurements → Pearson or Spearman; predictors → linear or
logistic regression. Alternatives run as sensitivity checks. Local files: only aggregates (n,
mean, SD, median, IQR — never minimum, maximum, or rows); identifier columns are refused; groups
smaller than the suppression threshold are refused; table cells are suppressed with complementary
suppression.

Edge cases: when the data show no difference at all (every value tied, or groups identical), the
rank tests give p = 1 and the t tests and ANOVA a statistic of 0 with p = 1, rather than an
undefined value. Rows and columns of a table that total zero do not count toward the chi-square
degrees of freedom or Cramér's V. When Fisher's exact test is asked for but cannot be calculated
(an empty row or column, or more than 1,000,000 possible tables), the chi-square test is shown
with the reason. A correlation with a variable whose values are all the same is refused. For
logistic regression, `warnings` reports separation (a predictor that predicts the outcome
perfectly or almost perfectly) or a model that did not converge, and the interpretation then says
not to interpret the estimates.

Golden tests: R `t.test`, `wilcox.test`, `oneway.test`, `kruskal.test`,
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
and markdown to embed the image with `export_document`. `log_scale` is refused for bar charts
(bars start at zero, which a log scale cannot show). A suppressed value's confidence limits are
hidden too, in the image and in the data table.

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
keeps them, locally), and the center line, control limits, and phase averages are then withheld
from the response as well (they would let the hidden values be worked out); the chart file has
them. In the shift rule a point exactly on the center line neither counts toward a run nor breaks
it, and in the trend rule a repeated value is skipped, so a flat series is never a trend
(Provost and Murray). Golden tests: R `qcc` limits.


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
exposure). Tuberculosis is not computed (refer). Disease names are not guessed: a name that fits
more than one disease ("hepatitis", "h flu"), names a syndrome ("gastroenteritis"), or excludes
the disease ("non-cholera Vibrio") is an error listing the candidates. For a rule counted in hours
(for example 24 hours after treatment starts), `as_of` reports the person as excluded through the
last day before the earliest return date. Dates must be YYYY-MM-DD between 1900 and 2100.
Decision support.

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
are masked as `<5`, but the ratios are computed from all records (the summary says so). With fewer
cases with an onset date than the suppression threshold, the median onset and the probable
exposure window are not shown (they would give away individual onset times). Exposures where no
one was recorded as exposed, or no one as unexposed, are listed in a caveat instead of being
dropped silently (a blank cell is not read as "no"). A `disease` the server's table does not hold
as one specific disease ("GI illness, agent unknown") is not an error: no incubation period is
used, and a caveat says how to give one. A time column is used as the onset time only when its
name says so ("Onset Time"); a column such as "Meal Time" must be named in `onset_time_column`.
An ill yes/no column is preferred to a generic "Status" column as the case column.

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
Field Epidemiology Manual, chapter 6; CDC Quick-Learn "Create an Epi Curve"). Onset and exposure
dates must fall between 1900 and 2100. A disease the server cannot match to one incubation period
is treated as no incubation period (the interval and pattern then come from the onsets alone).

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
each contact's first and last exposure dates, plus a sheet of the rules and sources. Each
exclusion column names who it applies to: the settings and every condition, including the "not"
ones (for pertussis, only health care workers who have no symptoms and no prophylaxis; for
hepatitis A, susceptible food handlers without prophylaxis), so a rule for one group is not read
as applying to every contact. No identifiers pass through the chat. Decision support.

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
DPH 2022 measles chapter and the 2013 ACIP statement). An infant under 12 months has no evidence
of immunity whatever dose count is given (doses before the first birthday do not count; the
reasoning says so). When the case's rash onset is given and the exposure dates extend outside the
infectious period (4 days before through 4 days after rash onset), PEP deadlines, exclusion, and
monitoring are counted from exposure within that period, and the answer says so. Decision
support.

### `detect_aberrations` (CD-10, toolset `cd`)

| Argument | Default | |
|---|---|---|
| `counts` | (required) | At least 8 periods, oldest first. |
| `frequency` | `weekly` | `daily`, `weekly`, or `monthly`. |
| `period_labels` or `start_date` | | Labels for the periods. |
| `series_name` | | What is being counted, e.g. "ILI emergency department visits". |
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
follow general-population guidance). Pathogens are not guessed: a name the content does not cover
("ebola", "legionella") is an error rather than the closest entry. Decision support.

### MCP prompt: `investigate_outbreak`

Arguments (all optional): `disease`, `setting`, `line_list_path`, `exposure_datetime`. Returns
one message that walks the model through the ten steps of a field investigation (CDC Field
Epidemiology Manual, chapter 3) adapted to a Massachusetts board of health — notify DPH
Epidemiology (617-983-6800), confirm the diagnosis and the outbreak (`detect_aberrations`), define
and count cases (`analyze_outbreak_line_list`), describe time, place, and person
(`generate_epi_curve`), start control measures (`calculate_isolation_and_exclusion`,
`infection_control_for_congregate_settings`, `plan_contact_tracing`, `assess_measles_exposure`),
test hypotheses (`analyze_two_by_two`), and communicate (`export_document`, MAVEN as DPH
directs) — with privacy rules for the whole investigation. The message names the toolsets the
workflow uses (`cd`, `epi`) and tells the model what to do when one is off. It turns the free-text
`setting` into the matching values of `calculate_isolation_and_exclusion`'s `settings` argument.
When `disease` is empty, it tells the model not to pass a syndrome ("GI illness") as a disease
name until the agent is known.

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
reachable; the result says so. A median household income the Census Bureau publishes at the
end of its range is shown as "$250,000 or more" (or "$2,500 or less"), not as $250,001.
Read-only; open-world.

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
headings are checked against the MeSH database (unknown ones are dropped and reported; an element
left with no terms is left out of the query with a warning, and a search with no terms left is an
error), and
PubMed's automatic term mapping adds the heading for the whole phrase. Elements are ANDed with the
filters. Returns the exact query (and PubMed's translation and a link to rerun it), the total
count, and for each record the PMID, title, journal, year, first three authors, an abstract
excerpt, DOI, publication types, and link. NCBI E-utilities (`esearch`, `efetch`) with
`tool=public-health-nursing-mcp-server` and no email; requests are spaced to stay under NCBI's
limit (3 per second, 10 with `PHN_NCBI_API_KEY`) and cached for a day, so a recent search also
works offline. An error reported by NCBI (for example a search backend timeout) is a tool error
and is not cached. Read-only; open-world.

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
evaluated are listed in the output file, not the answer. Roster columns may carry a dose number
("MMR 1", "DTaP #2", "Polio dose 3"); the answer lists the columns read as vaccines
(`vaccine_columns`) and those not used (`columns_not_used`: a requirement whose vaccine is in an
unused column shows as not met). In the Exemptions cell, a bare "medical" or "religious" covers
every vaccine; a cell that cannot be read sends the row for review instead of guessing.

**Needs review** is also used for a student 7 or older whose DTaP doses do not meet the K–6
requirement (Tdap or Td at 7 or older may complete a catch-up series, which the content does not
encode; DTaP is never suggested at 7 or older), and for a requirement met only with a dose given
less than the minimum interval after a dose that did not count (CDC spaces the repeat dose from
the invalid one).

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
Readings entered in Fahrenheit are compared with the Fahrenheit range (36–46 °F for a
refrigerator), so 46.3 °F is out of range.

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
rate limit and is never shown or saved in the cache. When RxNorm has no exact match, a caveat names
RxNorm's closest match and says which name the other sections were searched for (the close match,
or the name as typed when the match may be a different drug).
Shortages are also searched under the RxNorm ingredient names, because shortage records are filed
by generic name. A section that could not be checked says whether offline mode was on or the
service could not be reached; "not found" (HTTP 404) is reported as no results. There is no
offline copy: offline or unreachable sections are
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
Doses dated before the person's date of birth or after the assessment date are not evaluated:
they are counted (`dose_rows_outside_dates`) and listed in the dose-evaluation file, and
`dose_rows` counts only the doses evaluated. People whose rows give more than one date of birth
are counted (`persons_with_conflicting_birth_dates`); the first date was used.

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
omits series with no reported cases. Counts are provisional; DPH (MAVEN) counts can differ. In a
53-week MMWR year, week 53 is compared with week 52 of 52-week baseline years, and the "last 4
weeks" window is 4 weeks long in every year.

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

School health tools are in the `school` toolset (on in the `school_nurse` profile). Their content
is in `content/school/` (pending review, due 2027-02-01). The school year runs July 1 to June 30.

**Templates and translations.** Tools that write letters or plans for families (SCH-02, SCH-04,
SCH-05) ship English templates only. An agency adds its own approved translations as
`/data/templates/<language>/<kind>.md`, where `<language>` is a language name or code found in the
nurse's file (`spanish` or `es`, `portuguese` or `pt`, `haitian_creole` or `ht`, ...). For a
language with two words, name the folder `haitian_creole` (recommended), `haitian-creole`, or
`ht`. Other tools that read `/data/templates/` match folder names in slightly different ways;
ADR 0010 lists them. Kinds:
`vision_referral`, `hearing_referral`, `postural_referral`, `concussion_caregiver_letter`, and
`<condition>_emergency_plan` (`diabetes`, `asthma`, `seizures`, `food_allergy`). Each English
template is written next to the documents as `template_english_<kind>.md`; keep its
`{{placeholders}}`. Letters for a language without an agency translation are written in English
and counted in the answer. Nothing is machine-translated.

### `get_ma_school_screening_requirements` (SCH-01, toolset `school`)

| Argument | Default | |
|---|---|---|
| `screening` | all | `vision`, `hearing`, `bmi` (height and weight), `postural` (scoliosis), `sbirt`, `kindergarten vision certificate`, or `physical exam` (aliases accepted). |
| `grade` | all grades (table) | `PK`, `K`, `1`–`12`, or `ungraded` (then `age_years`). |
| `age_years` | | Age for ungraded classrooms (105 CMR 200.000 gives age rules). |
| `new_student` | false | New to the district: school-entry rules apply (DPH advises screening new students' vision). |
| `include_details` | true | Protocol, referral, notification, exemptions, reporting, and confidentiality. |

Rules come from 105 CMR 200.000 (vision 200.400(B), hearing 200.400(C), BMI 200.500, physical
examinations 200.100), M.G.L. c. 71, section 57 (postural screening annually in grades 5–9; the
kindergarten vision certificate) and section 97 (SBIRT), and DPH's 2022 vision protocol (critical
lines, charts, near vision, stereoacuity, instrument screening; public preschool screened
annually). Each band is `annual`, `once_in_band`, `school_entry`, `district_selected`, or
`entry_and_interval`. Hearing protocol details come from DPH's 2007 manual because the current DPH
hearing protocol could not be retrieved (marked medium or low confidence).

### `track_school_screenings` (SCH-02, toolset `school`)

| Argument | Default | |
|---|---|---|
| `path`, `sheet` | required | Roster with results (CSV/XLSX in `/data`), one row per student. |
| `as_of` | today | Sets the school year. |
| `screenings` | every one with columns | `vision`, `hearing`, `bmi`, `postural`, `kindergarten vision certificate`. SBIRT is refused. |
| `write_letters` | false | Referral letters for students referred whose follow-up is not documented (vision, hearing, postural). |
| `letter_format` | `docx` | `docx` or `pdf`. |
| `school_name`, `nurse_name`, `nurse_phone` | | Letter fields (or a `School` column). |

Columns: `Grade` (required); optional `Date of Birth`, `Sex`, `Home Language`, `New Student`,
`Parent Name`, `Student Name` or `First Name`/`Last Name`; and per screening a date, a result
(`pass`, `refer`, `absent`, `refused`, `exempt`), and a follow-up column, e.g. `Vision Date`,
`Vision Result`, `Vision Referral Completed`. BMI uses `Height (in)` and `Weight (lb)` (or cm/kg),
`BMI Percentile`, or `BMI Category`; with height, weight, date of birth, and sex the tool computes
the CDC BMI-for-age percentile (LMS method, `content/school/cdc_bmi_for_age.yaml`). A screening
counts toward this year when it falls in the window its band needs (this school year for annual
bands; since the student entered the band for once-per-band screenings). Statuses: `passed`,
`referred, follow-up pending`, `referred, follow-up done`, `incomplete`, `due`, `exempt or opted
out`, `not required`. The answer has suppressed counts and completion, referral, and follow-up
rates by grade (rates hidden when a count they use is hidden) and BMI category counts by grade
and sex. Per-student results and an action list go to
`/data/output/school_screenings_<date>.xlsx`; letters to
`/data/output/school_screening_letters_<date>/`. Columns that look like SBIRT results are ignored
and not copied (M.G.L. c. 71, section 97(c)).

Results such as "negative", "no concerns", or "no referral needed" count as a pass; a bare "no"
means not screened. Heights written in feet and inches (4'10") or with fractions are not read
(give inches or centimeters as a number); height and weight columns may use different units (cm
with lb, for example). The language and new-student columns are found only by their exact names
(`Home Language`, `Primary Language`, `Family Language`, `Preferred Language`, or `Language`;
`New Student`, `New Entrant`, `New to District`, or `New`), so a column such as "Speech/Language Services" is not taken for one. The counts by grade hide
more cells than before: required = screened + incomplete + exempt + due and referred = completed
+ pending are protected as sums too, so a hidden count cannot be found by subtraction, and a rate
is hidden when any count in its numerator or denominator is. Languages that the server does not
recognize are reported (and named in letter file names) as "Other"; text from a language cell is
never returned. Letter files are never overwritten: a number is added.

### `school_medication_administration_guide` (SCH-03, toolset `school`)

| Argument | Default | |
|---|---|---|
| `question` | | A question in plain language. |
| `topic` | | A topic id instead (listed when both are omitted). |
| `max_topics` | 4 | Topics returned for a question (1–6). |

Answers from 105 CMR 210.000 as amended 12/5/2025 (Mass. Register #1562) and M.G.L. c. 71,
section 54B, each point cited by section. Topics: scope and definitions, policies, DPH
registration (MCSR), delegation, orders and consent, the medication administration plan, field
trips, self-administration and self-carry, training, storage, documentation, medication errors,
epinephrine, opioid antagonists, other emergency rescue medications, and the nurse's judgment,
waivers, and inspections. Topics are matched by keywords, with keywords shared by several topics
counting less.

### `build_individualized_health_plan` (SCH-04, toolset `school`)

| Argument | Default | |
|---|---|---|
| `condition` | required | `diabetes`, `asthma`, `seizures` (epilepsy), or `food allergy` (anaphylaxis). |
| `student_file`, `sheet` | | Local fill: one row per student (CSV/XLSX in `/data`). |
| `format` | `docx` | `docx` or `pdf`. |
| `include_504` | true | Suggested Section 504 accommodations. |
| `write_templates` | false | Write the blank English templates to `/data/output`. |
| `school_name`, `nurse_name`, `nurse_phone` | | Fields for filled plans. |

Returns the plan as cited sections (recognize, emergency steps, call 911, nursing assessment,
goals, interventions, medications, required plan elements, suggested 504 accommodations, family
resources) and the placeholders. With `student_file`, columns such as `Student Name`, `Date of
Birth`, `Grade`, `Parent Name`, `Parent Phone`, `Emergency Contact`, `Provider Name`, `Provider
Phone`, `Allergies`, `Medications`, `Emergency Medication`, `Home Language`, and condition fields
(`Target Range`, `Low Threshold`, `Glucagon`, `Insulin`; `Peak Flow`, `Triggers`; `Seizure
Description`, `Rescue Instructions`; `Allergens`, `Epinephrine`) fill one IHP plus emergency action
plan per student in `/data/output/health_plans_<condition>/`; rows whose `Condition` names another
condition are skipped. A `Condition` cell may name several conditions ("asthma, peanut
allergy"); the student then gets a plan for each condition asked for. The answer has counts,
blank-field counts, and languages only (unrecognized languages as "Other"). Sources: CDC
school health and clinical pages, CDC's 2013 food allergy guidelines, 105 CMR 210.000, M.G.L. c.
71, section 54B, and DPH's 2007 manual (IHP process).

### `concussion_return_to_learn_tracker` (SCH-05, toolset `school`)

| Argument | Default | |
|---|---|---|
| `injury_date`, `diagnosis_date`, `returned_to_school`, `symptom_free_date`, `full_academics_date`, `cleared_to_begin_rtp` | | Dates for one student (no name). |
| `during_extracurricular` | true | Injury in an extracurricular athletic activity (105 CMR 201.000 applies). |
| `rtp_stage_completed` | 0 | Highest gradual return-to-play stage completed (DPH clearance form, 1–6). |
| `symptoms` | | Plain words, to pick accommodations from DPH's Table 1. |
| `symptom_ratings` | | DPH Post-concussion Symptom Monitoring Tool ratings (0–6 for 22 symptoms). |
| `rest_guidance` | `dph2018` | `dph2018` or `cdc2025` for time out of school. |
| `as_of` | today | |
| `tracker_path`, `sheet` | | Log mode: a head injury log in `/data`. |
| `write_plans` | false | Log mode: filled reentry plans and caregiver letters. |
| `format`, `school_name`, `nurse_name`, `nurse_phone` | | Document options. |

One student: a timeline (removal from play, parent contact, written notice and athletic
director/nurse notification by the end of the next business day, Report of Head Injury form,
reentry plan, the 4-week mark), DPH's return-to-learn stages, the DPH clearance form's six
return-to-play stages with status, accommodations, a symptom-check schedule (twice weekly for two
weeks after return, then weekly), forms with links and translations, danger signs, and flags
(clearance before full-time classes without accommodations, stages without a symptom-free date or
clearance, 4 weeks without a full academic load). Log mode checks each record (written notice and
AD/nurse notice on time, form on file, reentry plan, clearance before return and after full
academics, clearing provider type) and returns suppressed counts, including the 105 CMR 201.017
statistics; per-student checks go to `/data/output/concussion_log_check_<date>.xlsx`. A notice
column marked "yes" without a date counts as done, date not recorded: it is not a gap, and
`notices_without_date` counts these records. A "Re-entry Plan" column is recognized, and the
language column is found only by its exact name. Business days skip weekends, not holidays;
symptom-check dates that fall on a weekend move to the next weekday.

### `monitor_school_absenteeism` (SCH-06, toolset `school`)

| Argument | Default | |
|---|---|---|
| `path`, `sheet` | required | Absences in `/data`: one row per absent student-day (Date, School, Reason) or daily counts (Date, School, Count, Reason or syndrome columns such as `ILI` and `GI`, Enrollment). |
| `date_column`, `school_column`, `reason_column`, `count_column`, `enrollment_column` | found by name | Overrides. |
| `group_column` | | Also look within grades, classrooms, or teams (at most 20 values; identifier columns refused). |
| `syndromes` | `ili`, `gi`, `all_illness` | Also `rash`, `other_illness`, `total`. |
| `end_date` | last date in the file | |
| `report_days` | 5 | School days shown and checked for signals. |

Reasons are sorted into influenza-like illness (CDC ILINet definition), gastrointestinal illness
(DPH: vomiting or diarrhea above what is normal for the facility), rash, and other illness by
keywords. EARS C1, C2, and C3 (`phn_epi::aberration`, as in `detect_aberrations`) run on each
school's (and group's) daily series over the school days in the file, with a baseline SD floor of
0.5 when counts are sparse. The answer lists signals, recent days (counts from 1 to the threshold
shown as `<n`), and the notification rules: 105 CMR 300.134 (report a suspected cluster to the
local board of health immediately), DPH's online gastrointestinal cluster form for schools and
child care, and 606 CMR 7.11(9) for child care. Exact counts go to
`/data/output/school_absenteeism_<date>.xlsx`. Names and IDs in the file are never read.

### `build_school_health_annual_report` (SCH-07, toolset `school`)

| Argument | Default | |
|---|---|---|
| `visit_log_path`, `visit_log_sheet` | | Health office visits: Date, Reason, Disposition; optional Grade, Type (student/staff), 911 Type, Injury, Head Injury, Student ID (used only to count distinct students). |
| `medication_log_path`, `medication_log_sheet` | | Doses: Date and Type (scheduled or PRN). |
| `screening_path`, `screening_sheet` | | The roster `track_school_screenings` reads. |
| `school_year` | the school year that contains `as_of` | e.g. `2025-2026`. |
| `as_of` | today | Evaluation date, YYYY-MM-DD; it sets the default school year. |
| `enrollment` | | For per-student rates. |
| `school_name`, `summary_format` | | Report title; `docx` or `pdf`. |

Visits are sorted into DPH's published categories (health maintenance, injury/first aid,
mental/behavioral health, miscellaneous) and dispositions (returned to class, dismissed for
illness or injury, 911 medical or behavioral, other). The workbook
`/data/output/school_health_annual_<year>.xlsx` has monthly activities, medication doses,
screenings (screened, referred, completed referrals), the BMI report (grade × sex × category)
with DPH's height and weight data-quality ranges, the DPH reports a school may owe, and
definitions; a school committee summary (`docx` or `pdf`) has suppressed aggregates and DPH's
2011–2012 statewide returned-to-class rate for comparison. DPH does not publish its current Status
Report form, so field names follow DPH's published categories; match them to DPH's forms.

When `school_year` is not given and no visit or medication row is dated in the school year that
contains `as_of` (for example a report run in July or August), the report covers the school year
before it and says so. Visit reasons and dispositions are matched by whole words or word starts,
never inside a word ("distress" is not "stress", "heartburn" is not a burn), and "called home;
returned to class" counts as a return. A year percentage is left out when a count behind it is
suppressed. Without a `911 Type` column, 911 calls are sorted as medical or behavioral health from
the visit reason, and a note says so.

### `child_care_health_consultation_checklist` (SCH-08, toolset `school`)

| Argument | Default | |
|---|---|---|
| `program_type` | `large_group` | `family`, `small_group`, or `large_group`. |
| `domains` | all | `health_policy`, `consultant`, `medication`, `care_plans`, `records`, `illness`, `infection_control`, `first_aid`, `safe_sleep`, `nutrition`, `staff_health`, `safety`. |
| `findings` | | `[{item, status, note}]`, status `met`, `partial`, `not_met`, or `not_applicable`. |
| `write_worksheet`, `worksheet_format` | false, `xlsx` | A visit worksheet in `/data/output`. |
| `program_name` | | Worksheet title. |

Items paraphrase 606 CMR 7.00 (Mass. Register #1459, 12/24/21) with the section cited and the
evidence to look for, filtered by program type (for example the health care policy and
consultant approvals in 7.11(19) apply to large group programs). Findings give counts by domain
and a gap list.

<!-- End of Phase 7 tools. -->

## Phase 8: Maternal, child, and family health

<!-- Phase 8 tools go below this line. -->

### `calculate_growth_percentiles` (MCH-03, toolset `mch`)

Percentiles and z-scores for a child's weight, length or height, BMI, and head circumference,
using the charts CDC recommends: the **WHO 2006 growth standards from birth to 24 months** and
the **CDC 2000 growth charts from 2 to 20 years**, with CDC's **2022 extended BMI-for-age**
method above the 95th percentile.

| Argument | |
|---|---|
| `sex` | `male` or `female` (required for one child). |
| `measurements` | One or more visits, oldest first. Each visit gives an age (`age_days`, `age_months`, `age_years`, or `measurement_date` with `birth_date`) and at least one of `weight_kg` (or `weight_lb` + `weight_oz`), `length_or_height_cm` (or `_in`), `head_circumference_cm` (or `_in`); optional `position` (`recumbent` or `standing`). At most 60 visits. |
| `birth_date` | Only needed with `measurement_date`. Dates are used to compute age and are not returned. |
| `gestational_age_weeks`, `gestational_age_days` | Gestational age at birth. Under 37 weeks, corrected age is used until 24 months of chronological age (`correct_for_prematurity: false` turns this off). |
| `measurements_path`, `sheet` | Batch mode: a CSV/XLSX in `/data` with one row per child-visit (`sex`; `age_months`/`age_days`/`age_years` or `date_of_birth` + `measurement_date`; `weight_kg`/`weight_lb`; `height_cm`/`height_in`; `head_circumference_cm`; `gestational_age_weeks`). |

**Method** (CDC's SAS programs and data-file pages):

- z = ((X/M)^L − 1)/(L·S); percentile = Φ(z). L, M, S are interpolated linearly: WHO by day of
  age (CDC's daily WHO file) and by 0.1 cm of length; CDC between the half-month rows.
- WHO weight, BMI, and weight-for-length z-scores beyond ±3 use the distance between the 2 and
  3 SD values (WHO's restricted method). CDC uses ln(X/M)/S when |L| < 0.01.
- Extended BMI (from 2 years): above the 95th percentile, percentile = 90 + 10·Φ((BMI − P95)/σ),
  with σ = 0.3728 + 0.5196·age − 0.0091·age² (boys) or 0.8334 + 0.3712·age − 0.0011·age² (girls),
  age in years; z = Φ⁻¹(percentile/100). The 2000-chart LMS z-score is also returned.
- Flags: WHO values at or below the 2.3rd or at or above the 97.7th percentile (±2 SD, MMWR
  2010); CDC values at or below the 5th or at or above the 95th; CDC BMI categories (underweight,
  healthy weight, overweight, obesity, severe obesity class 2 at ≥120% of the 95th percentile or
  BMI ≥35, class 3 at ≥140% or BMI ≥40); extreme values to re-measure (WHO z cut points and CDC
  modified z-scores from CDC's SAS programs).
- Several visits show the change in z-score and in the measurement since the previous visit.
- Age 20 or older, length outside 45–110 cm for weight-for-length, head circumference after 36
  months, and a corrected age before term are reported as "not scored" with the reason.

**Batch mode** writes `growth_percentiles.xlsx` (per-row percentiles, z-scores, BMI, weight
status, flags, and rows not scored, with any name and ID columns copied from the file) to the
output folder and returns only weight-status counts by age group (WHO weight-for-length under 2;
CDC BMI category from 2), with small-cell suppression.

**Golden tests** (`crates/phn-tools/tests/growth_golden.rs`) reproduce every published percentile
in CDC's 2000 data files (weight, stature, head circumference; relative error < 1e-7), all
percentiles and z-score values in CDC's extended BMI file (to its 4 decimals), CDC's four worked
extended-BMI examples, and WHO's published monthly L, M, S and ±3 SD values from the daily file.

Sources: snapshots `who-growth-standards` (CDC's `WHOref_d.csv`) and `cdc-growth-charts` (CDC
data files, verbatim); content `mch-growth-interpretation` (pending review). The corrected-age
rule is **low confidence** (no CDC source states it; confirm the agency's rule).
### `childhood_lead_screening_guide` (MCH-04, toolset `mch`)

Massachusetts childhood lead screening and follow-up from **105 CMR 460.000** (Mass. Register
#1353, 12/1/17) and DPH's Childhood Lead Poisoning Prevention Program (CLPPP) pages.

| Argument | |
|---|---|
| `child_age_months` | Child's age: each required screening is shown as due now, upcoming, or past. |
| `town` | Where the child lives: DPH's current high-risk list (rank) and the town's published screening rate and blood lead prevalence with state values. |
| `blood_lead_level`, `sample_type` | A result in µg/dL and `capillary` or `venous` (required with a level). |
| `risk_factors` | `household_poisoned`, `renovation`, `deteriorated_paint`, `clinical_judgment` (105 CMR 460.050(D)). |
| `teaching_sheet`, `language` | Write a family teaching sheet (DOCX) to the output folder; see translations below. |

**What it returns:** definitions (lead poisoning ≥10 µg/dL venous; blood lead level of concern
5–<10; CDC reference value 3.5); the routine schedule (9–12 months, 2, 3, and 4 in a high-risk
community; entry to child care, pre-K, or kindergarten); high-risk schedules; venous
confirmation timing for capillary results (3.5–4.9 and 5–9: 3 months; 10–14: 1 month; 15–19:
2 weeks; 20–44: 1 week; 45–69: 48 hours; ≥70: emergency care); provider, environmental, case
management, and re-test actions by venous level (DPH's table); reporting (lead poisoning within
3 working days; every result electronically within one week; confidentiality); the Lead Law;
family teaching; CLPPP contacts and pediatric environmental health clinics.

**High-risk communities:** the 17 communities on DPH's 2024 list (2020–2024 data, from the 2024
Annual Childhood Lead Poisoning Surveillance Report), checked against snapshot
`ma-lead-screening`. Update `content/mch/lead_screening.yaml` and the snapshot each year
(`xtask/scripts/build_lead_snapshot.py`).

**Translations:** the server ships an English teaching sheet only. For another language, put
the agency's approved translation at `/data/templates/<language>/lead_family_teaching.docx` (used
as is) or `.md` (rendered with your letterhead). `<language>` is the folder name you choose, for
example `spanish` or `es` (common codes and names are both tried). For a language with two
words use `haitian_creole` (recommended), `haitian-creole`, or `ht`. If none is found, the
English sheet is written and the result says so; the server does not machine-translate.

Content: `mch-lead-screening` (pending review). DPH's follow-up table combines Massachusetts
requirements with CDC's 2021 reference value and CDC's schedules; the tool says so.

### `mandated_reporter_51a_guide` (MCH-07, toolset `mch`)

Massachusetts reporting of suspected child abuse and neglect under **M.G.L. c. 119, s. 51A**
(and the s. 21 definition of mandated reporter), with DCF's guidance.

| Argument | |
|---|---|
| `topics` | Any of `who`, `when`, `how`, `contents`, `protections`, `definitions`, `domestic_violence`, `process`, `warning_signs`, `documentation` (default all). |
| `concerns` | Kinds of concern to tailor the guidance: `physical_abuse`, `sexual_abuse`, `emotional_abuse`, `neglect`, `sexual_exploitation`, `human_trafficking`, `prenatal_substance_exposure`, `domestic_violence`, `child_death`. No names or details. |
| `works_for_facility` | Adds the option to notify the person in charge (s. 51A(a)). |
| `oral_report_time` | When DCF was called (`2026-11-08 14:30`): the written report is due 48 hours later. |
| `narrative_template` | Return the written-report template: sections and field names following DCF's Child Abuse Reporting Form. |
| `notes_path`, `sheet` | The nurse's notes in `/data`: a `.txt`/`.md` file of `Field name: value` lines (a value may continue on following lines; lines that do not start with a field name stay with the previous field) or a CSV/XLSX with one row per report and field names as columns. |

**Template + local fill.** The template (no identifiers) is returned to the model. With
`notes_path`, the tool fills `51a_written_report_draft.docx` per report in the output folder:
each template field, `?` for unknown entries (as DCF's form asks), DCF's related-concerns
checklist with the named items checked, and a signature line. The response lists only file
names, the number of fields filled, and the names of fields left blank; label and column
mismatches are counted, never shown. The privacy block lists the files and the identifier
categories detected in the notes.

**Content** (`mch-mandated-reporting`, pending review): who must report (nurses included); the
four reporting grounds; that prenatal substance exposure alone does not meet the requirement
(s. 51A(a)) and that DCF's guide still lists newborn drug dependence (flagged low confidence for
the nurse to reconcile); the steps (911 if in danger; call the DCF area office or the
Child-At-Risk Hotline 1-800-792-5200; written report within 48 hours online, by fax, or by mail;
the facility designee option; police, Child Advocate, and DA/medical examiner for deaths);
report contents (s. 51A(d) and DCF's additions); penalties and good-faith protections; DCF's
definitions (110 CMR 2.00); domestic violence guidance; DCF's screening and response timelines;
warning signs; and documentation tips.
### `check_developmental_milestones` (MCH-02, toolset `mch`)

CDC's "Learn the Signs. Act Early." milestones (the 2022 CDC/AAP revision, as published on
cdc.gov, pages updated 2026-05-16) and Massachusetts Early Intervention referral.

| Argument | |
|---|---|
| `age_months` | Child's age (2 months to 5 years; the checklist for the most recent age reached is used: 2, 4, 6, 9, 12, 15, 18, 24, 30, 36, 48, 60 months). |
| `gestational_age_weeks` | Born before 37 weeks and under 24 months: the corrected-age checklist is shown too. |
| `not_yet` | IDs of milestones the child does not yet do, from the checklist (`18m-language-1`). |
| `lost_skills`, `concern` | Lost skills or other concerns (act early either way). |
| `town` | The EI programs that serve the town (snapshot `ma-ei-programs`, DPH's directory of 2026-09-24). |

**Returns** the checklist (ID, area, milestone, status) with CDC's source page and date; whether
to act early and CDC's steps (talk with the doctor and ask about screening; ask for a specialist
referral; request a free evaluation from EI under 3 or the public school district at 3 or older,
with no doctor's referral needed); screening due (AAP: general at 9, 18, 30 months; autism at 18
and 24 months, within 1.5 months of those ages, or now when there is a concern); Massachusetts EI
eligibility, referral (anyone can refer), and cost (none to families); EI programs for the town
with phone and website; CDC's questions to share with the doctor; and the next checklist age.
Milestones marked not yet on the corrected-age checklist count toward acting early too (a caveat
says how many). When the corrected age is under 2 months (before CDC's first checklist), only the
chronological-age checklist is shown, with a note to decide with the child's provider which age
to use.

Milestones are what 75% or more of children do by an age; the checklists are not a substitute
for standardized screening. CDC does not say how to adjust for prematurity; the tool shows both
checklists and defers to the child's provider. Content: `mch-developmental-milestones` (pending
review; milestones transcribed word for word).
### `plan_newborn_postpartum_visit` (MCH-01, toolset `mch`)

A newborn and postpartum home visit plan: what to check and what to teach.

| Argument | |
|---|---|
| `baby_age_days` | Baby's age at the visit (0-365). |
| `gestational_age_weeks` | For context (the AAP jaundice guideline covers 35 weeks or more). |
| `feeding` | `breast`, `formula`, or `mixed` (tailors the feeding points). |
| `birth_weight_g` (or `_lb` + `_oz`), `weight_g` (or `_lb` + `_oz`) | Percent change from birth weight (reported, not judged). |
| `discharged_within_24_hours` | Reminder that a second blood spot is due within 48 hours of birth. |
| `hearing_screen` | `passed`, `not_passed`, `not_done`, `unknown`: follow-up reminder. |
| `documents`, `language` | Write a visit plan (with check boxes and citations) and a family teaching sheet (DOCX) to the output folder. |

**Sections:** feeding and weight (CDC: 8-12 feeds a day, formula amounts in the first days,
vitamin D, hunger and fullness cues); jaundice (**low confidence**: the AAP 2022 guideline text
could not be retrieved, so only general points and the discharge plan); safe sleep (CDC's page
supporting the AAP 2022 policy and the policy abstract); car seat (M.G.L. c. 90, s. 7AA);
newborn screening (105 CMR 270.007: blood spot at 24-48 hours, second specimen after early
discharge, refusal only on religious grounds; M.G.L. c. 111, s. 67F hearing screen); postpartum
health with CDC Hear Her's urgent maternal warning signs (up to a year after delivery).

The plan and teaching sheet contain no information about the family. Translations follow the
same rule as the lead teaching sheet: `/data/templates/<language>/newborn_postpartum_teaching.docx`
or `.md`; otherwise English, and the result says so. Content: `mch-newborn-postpartum` (pending
review).
### `estimate_benefit_eligibility` (EQ-06, toolset `equity`)

An **income screen, not an eligibility determination**, for WIC, SNAP, MassHealth, the Health
Safety Net, and home energy assistance, from household size and gross income.

| Argument | |
|---|---|
| `household_size` | People in the household (1-20). |
| `annual_income` or `monthly_income` | Gross household income before taxes. |
| `pregnant`, `unborn_children` | MassHealth counts the unborn child or children in household size. |
| `child_under_5`, `infant_under_1`, `children_under_19`, `young_adult_19_20`, `postpartum_or_breastfeeding` | Which programs apply. |
| `receives_snap_masshealth_or_tafdc` | WIC automatic income eligibility. |
| `pays_for_heat` | HEAP applies to households that pay for heat (including heat in rent). |

**Limits** (content `equity-benefit-eligibility`, pending review): 2026 HHS poverty guidelines
(48 states and DC: $15,960 for one person plus $5,680 per additional person); WIC 185% of FPL
(DPH table, June 11, 2026 - June 30, 2027; automatic with certain MassHealth plans, SNAP, or
TAFDC); SNAP gross monthly income 200% of FPL and maximum benefits (DTA chart; assumed effective
October 1, 2026); MassHealth (Member Booklet, standards effective March 1, 2026): pregnant
people and infants under 1 at 200% (unborn counted), children 1-18 Standard at 150% and Family
Assistance under 19 at 300%, young adults 19-20 at 150%, parents and caretakers at 133%,
CarePlus adults 21-64 at 133%; Health Safety Net at 300% (no deductible up to 150%); HEAP at 60%
of state median income (FY 2027 table, households of 1-10).

**Returns** the percent of FPL, each program's limit for the household (monthly and annual),
the screen result (likely income-eligible, over the limit, automatically income-eligible, or
does not apply from the facts given), the tier (Family Assistance; HSN with or without a
deductible), SNAP's maximum benefit, notes, how to apply, and each program's effective date.
A unit test checks that the WIC and SNAP tables equal 185% and 200% of the poverty guidelines.
**Refresh annually:** FPL in January, MassHealth March 1, WIC June/July, SNAP and HEAP October 1.
### `map_lead_risk` (MCH-05, toolset `mch`)

Ranks cities and towns (or census tracts) for childhood lead outreach and draws a map.

| Argument | |
|---|---|
| `geography` | `town` (default) or `tract`. |
| `county`, `towns` | Limit the areas in scope (for tracts: tracts whose main town is listed). |
| `indicators` | `pre1950_housing`, `pre1980_housing`, `low_income` (below 200% of poverty), `children_under5`, and for towns `screening_gap` (100 − % of children 9-47 months screened) and `elevated_bll` (% of screened children with an estimated confirmed level of 5 µg/dL or more). Default: pre-1950 housing and low income, plus the two DPH screening measures for towns. |
| `top` | Areas to list (default 20, at most 100). |
| `map` | Draw a choropleth of the index (default true; SVG and PNG in `output/maps/`). |

**Method:** the outreach index is the mean of the indicators' percentile ranks (0-100, ties
averaged) among the areas in scope; an area missing a value (DPH's "NS") is ranked on the
others. It is this server's planning ranking, not a DPH method; DPH's high-risk designation and
its high-risk score (5-year incidence adjusted for low income and pre-1978 housing) are shown
next to it, and designated communities outside the top list are named.

**Data** (bundled snapshots): `ma-acs5-housing-age` (ACS 2020-2024 5-year B25034, built with
`xtask/scripts/build_housing_age.py` from the keyless summary file; pre-1950 and pre-1980 with
MOEs), `census-acs5-ma` (below 200% of poverty, under 5), and `ma-lead-screening` (DPH 2024
surveillance report: screening and prevalence by community, high-risk list). ACS bins do not
split at 1978. Statewide values are returned for comparison. No local data is read.
### `maternal_infant_health_profile` (MCH-06, toolset `mch`)

Births and maternal and infant health indicators for Massachusetts and its communities.

| Argument | |
|---|---|
| `town` or `county` | Community profile: 2023 resident births, low birthweight %, preterm %, teen births. |
| `by_race` | Statewide figures by race and Hispanic ethnicity (default true). |
| `trends` | State trends (default true). |

**Community indicators** are computed from DPH's counts (2023 Birth Report, Table 17) with exact
Poisson 95% intervals (`phn_epi::rates`), flagged as unstable below 20 events, suppressed for
DPH's 1-4 counts and below the configured threshold, and compared with the state ("higher" or
"lower" only when the interval excludes the state value) and the U.S. **Statewide by race and
Hispanic ethnicity** (2023): low birthweight, preterm, adequate prenatal care (APNCU),
first-trimester care, cesarean, in-hospital breastfeeding; infant, neonatal, and postneonatal
mortality (2022 Death Report, Table 5), each with the ratio to White non-Hispanic. **PRAMS**
(2019-2021 report, weighted % with 95% CIs): depression screening (prenatal, postpartum),
postpartum depressive symptoms, breastfeeding initiation and 8 weeks, back to sleep, no
bed-sharing, no soft bedding, unintended pregnancy. **Trends:** low birthweight, preterm, and
adequate prenatal care 2013-2023; infant mortality 2012-2022. **U.S.:** CDC NCHS provisional
estimates (data.cdc.gov `76vv-a7x8`, 12 months ending 2024 Q4; `jqwm-z2g9`, 12 months ending
2023 Q4), labeled with their periods.

Data: snapshot `ma-maternal-infant`, built from DPH's annual reports and CDC's datasets by
`xtask/scripts/build_maternal_infant.py` (CDC WONDER was blocked; DPH publishes no machine-readable
town file). Community infant mortality and PRAMS are not published. No local data is read.
<!-- End of Phase 8 tools. -->

## Phase 9: PHN practice toolkit and case management

<!-- Phase 9 tools go below this line. -->

Phase 9 tools support everyday case management and documentation. Every content file they use is
`status: pending` (review due 2027-02-01); see `docs/acceptance/phase-9.md` for the review
checklist. Narrative notes and client lists are read locally and never returned.

### `plan_tb_contact_investigation` (CD-11, toolset `cd`)

| Argument | Default | |
|---|---|---|
| `index_case` | required | Clinical facts only: `site` (`pulmonary`, `laryngeal`, `pleural`, `extrapulmonary_only`), `symptomatic`, `afb_smear_positive`, `cavitary`, `chest_radiograph` (`consistent_with_tb`, `abnormal_not_consistent`, `normal`, `not_done`), `naa_positive`, `culture_positive`, `mdr_tb`, `age_years`, and dates: `symptom_onset_date`, `first_positive_finding_date`, `suspected_diagnosis_date`, `treatment_start_date`, `isolation_start_date`, `infectious_period_end_date`. |
| `contact_groups` | | Counts and risk facts, never names: `setting` (`household`, `workplace`, `school`, `child_care`, `congregate_living`, `medical_procedure`, `health_care`, `social`, `travel`, `other`), `count`, `age_band` (`under_5`, `5_to_15`, `16_plus`, or `unknown`), `medical_risk`, `exceeds_high_priority_limits`, `exceeds_medium_priority_limits`, `last_exposure_date`, `prior_positive_test`. |
| `contact_file`, `sheet` | | A local contact list (columns such as Age or DOB, Household, Setting, Medical Risk, Last Exposure, Test1 Result, Test2 Result). |
| `listed_date` | today | Starts the Table 3 time frames. |
| `write_workbook`, `template_rows` | true, 50 | XLSX tracking workbook in `/data/output`. |

Applies the 2005 NTCA/CDC guidelines (MMWR 54(RR-15)): whether to investigate; the infectious
period (Table 2: 3 months before symptom onset or the first positive finding, whichever is
earlier; 4 weeks before the suspected diagnosis for asymptomatic, smear-negative, noncavitary
cases), the closing criteria, and the window to ask about; contact priority from the Figure 2-4
decision trees; the initial-encounter and evaluation deadlines in business days (Table 3); and
the testing schedule, including the retest 8-10 weeks after last exposure, window prophylaxis for
children under 5, and full treatment for immunocompromised contacts. Adds DPH TB program phone
numbers, reporting rules (TB disease immediately; positive LTBI tests online), and DPH's state TB
clinics (those in the configured county first). The workbook has Y/N validation and formulas for
priority, WORKDAY due dates, and the retest window. With `contact_file` the workbook is filled
locally and the answer returns only suppressed counts by priority, tested/positive counts, and
expansion signals (10% or more positive among high-priority contacts, infection under age 5,
negative-to-positive changes, disease in a low-priority contact).

A `symptom_onset_date` means the index patient had symptoms, so the infectious period counts from
3 months before onset even when `symptomatic` is left false (a caveat says so). In a contact file,
ages may be whole years or carry a unit ("8 months", "18 mo", "6 wk"), and dates of birth with a
year outside 1900–2100 are unreadable; contacts whose age could not be read are counted
(`ages_not_read`), because the age rules (under 5, for example) were not applied to them.

### `ltbi_treatment_calendar` (CD-12, toolset `cd`)

| Argument | Default | |
|---|---|---|
| `regimen` | required | `3HP`, `4R`, `3HR`, `6H`, `9H`, `6H_twice_weekly`, `9H_twice_weekly` (names such as "rifampin" also work). |
| `start_date` | required | First dose, YYYY-MM-DD. |
| `administration` | DPH's mode or self-administered | `dot`, `vdot`, or `sat` (twice-weekly regimens are DOT only). |
| `twice_weekly_days` | Monday, Thursday | Dosing days for twice-weekly regimens. |
| `doses_taken`, `as_of` | , today | Dose dates for the adherence check. |
| `write_files` | true | XLSX adherence tracker and DOCX patient calendar in `/data/output`. |
| `language` | English | Uses `/data/templates/<language>/ltbi_calendar.md` (placeholders `{{regimen}}`, `{{start_date}}`, `{{doses}}`, `{{completion_deadline}}`, `{{program_phone}}`) when present; otherwise English with a caveat. |

Every dose date (weekly and twice-weekly regimens), monitoring visits (DPH's weeks 1, 5, and 9 for
3HP; monthly otherwise), and the completion window: 11 of 12 3HP doses within 16 weeks with doses
more than 72 hours apart; 120 4R doses within 6 months; 180 (6H) or 270 (9H) daily doses within 9
or 12 months; 52 or 76 twice-weekly doses within 9 or 12 months (2000 ATS/CDC). No window is
published for 3HR. The adherence check counts valid doses, finds gaps over 2 months (TB disease
must be ruled out before resuming), and projects whether completion still fits the window. CDC
(2020) and DPH positions are both shown; DPH's page lists 3HP, 4R, and 9H. Doses are never
calculated (plan item CD-16).

### `screen_health_related_social_needs` (EQ-05, toolset `equity`)

| Argument | Default | |
|---|---|---|
| `answers` | list items | One person's answers by item id or question number (1-26, 23a, 23b): option keys or labels, numbers for items 7-10, 17, 18, 23a/b, a list for item 2. |
| `age_years` | | Sets the physical activity threshold and the age limits of items 25-26. |
| `path`, `sheet`, `group_by` | | Batch mode: a local file with one screening per row (columns by item id or q1-q26, optional Age); counts by an optional category column. |
| `list_items` | false | Item ids, answer keys, sources, and reuse terms. |

Scores the CMS AHC HRSN screening tool by domain using the answers CMS underlines: living
situation, food, transportation, utilities, interpersonal safety (HITS items 7-10 summing to 11 or
more), and the supplemental domains (financial strain, employment, family and community support,
education, physical activity under 150 minutes a week for adults or 60 minutes a day for ages 6-17,
substance use, mental health with PHQ-2 of 3 or more or any stress, disabilities). Positive
domains map to Massachusetts referral categories (EA family shelter, Mass 211, HomeBASE, SNAP,
WIC, HEAP, MassHealth ACO HRSN services, MassHire, 988). Item wording is returned only for
public-domain items. Batch mode writes per-person results to
`/data/output/hrsn_screening_results.xlsx` and returns suppressed counts by domain and group. With
`group_by`, each domain's counts for every group and "All" are suppressed together, so a count
hidden in one group cannot be found by subtracting the other groups from "All".

### `omaha_system_coder` (CORE-01, toolset `core`)

| Argument | Default | |
|---|---|---|
| `text` or `path` | | A narrative to code (text, or a note file in `/data`: TXT, MD, DOCX, PDF, HTML). Never returned. |
| `lookup` | | A problem, target, or intervention category by name or code. |
| `ratings` | | Knowledge, Behavior, Status (1-5) per problem, with optional baseline ratings. |
| `max_problems` | 8 | |
| `write_care_plan` | false | Blank Omaha System care plan (DOCX) for the suggested problems. |

Suggests Omaha System problems (42 in 4 domains, with codes, suggested modifiers, and
signs/symptoms), intervention category and target pairs found in the same sentences, and the
Knowledge-Behavior-Status scale. A problem whose terms appear only after a negation word ("denies
fever", but also "no income" or "not taking insulin") is not in `problems`: it is listed in
`mentioned_as_absent_or_negated` for the nurse to decide, because an absent need is often the
problem itself. The result has
codes and the vocabulary terms that matched, never the note's words. Signs/symptoms not seen on
omahasystem.org are marked unverified.

### `classify_intervention_wheel` (CORE-02, toolset `core`)

| Argument | Default | |
|---|---|---|
| `activities` | list the Wheel | Short activity descriptions (returned by number only). |
| `path`, `sheet`, `activity_column`, `hours_column`, `group_by` | | Classify an activity log; per-activity results go to `/data/output`; counts and hours by intervention, level, and group are returned with suppression. |

Classifies activities by the 17 interventions, 5 wedges, and 3 levels of practice of the Public
Health Intervention Wheel, with definitions and confidence (MDH's own page could not be fetched;
the definitions of four interventions (surveillance, screening, delegated functions, and
consultation) and of the three levels of practice are low confidence).

### `nursing_documentation_template` (CORE-03, toolset `core`)

| Argument | Default | |
|---|---|---|
| `format` | `soap` | `soap`, `dar`, or `narrative`. |
| `visit_type` | `home_visit` | `home_visit`, `clinic`, `case_investigation`, or `dot_visit`. |
| `check_path` | | A note in `/data` to check for the required elements (text never returned). |
| `skip_elements` | | Element ids to leave out (for example `consent`). |
| `write_template` | false | DOCX and Markdown template in `/data/output`. |

Section prompts, visit-specific prompts, and required elements: date and time, signature with
credentials (244 CMR 9.03(44)), consent, language and interpreter, assessment, interventions,
teaching with teach-back, response, goals, follow-up, and visit-specific items (safety for home
visits; reporting, control measures, and contacts for case investigations; dose observed and
side-effect screen for DOT). 42 CFR 484.110 and 484.50 are cited as a reference standard. The
note check returns found/missing per element, the section where each was found, and counts of
identifier kinds.

### `find_community_resources` (EQ-07, toolset `equity`)

| Argument | Default | |
|---|---|---|
| `town` | configured town | Massachusetts city or town. |
| `service` | | Category (food, housing, utilities, transportation, primary_care, dental, behavioral_health, substance_use, safety, legal_immigration, employment, education, child_family, older_adults, disability) or any words. |
| `language`, `open_on`, `open_at` | | Language offered; weekday or "today"; HH:MM. |
| `include_county`, `include_health_centers` | true, true | |
| `directory_path`, `sheet` | `resources/community_resources.csv` | The nurse's directory. |
| `max_results` | 15 | At most 50. |

Searches the nurse's directory (columns: name, services, town, address, phone, website,
languages, hours such as "Mon-Fri 9:00-17:00; Sat 10:00-14:00" or "24/7", eligibility, cost,
last_verified) and HRSA-funded health center sites (data.hrsa.gov download filtered to
Massachusetts, cached a week, snapshot `hrsa-health-centers-ma` offline). A health center listed
under a village or neighborhood ("Dorchester", "Hyannis") is placed in the main town of its ZIP
code and shown as "Dorchester (Boston)". Ranks same town, then
same county; flags entries verified more than 12 months ago. Files that look like client lists
(names, dates of birth, record numbers) are refused.

### `newcomer_health_navigator` (EQ-11, toolset `equity`)

| Argument | Default | |
|---|---|---|
| `topics` | all | `screening`, `programs`, `coverage`, `shelter`, `all`. |
| `status` | | Immigration status for coverage (refugee, asylee, cuban_haitian_entrant, special_immigrant_visa, trafficking_victim, withholding_of_removal, cofa_migrant, lpr_5_years_or_more, lpr_under_5_years, parolee_1_year_plus, parolee_under_1_year, temporary_protected_status, asylum_applicant_with_work_permit, asylum_applicant_no_work_permit, daca, nonimmigrant_visa, undocumented). |
| `age_years`, `pregnant`, `disabled` | | Age rules for screening and coverage. |
| `arrival_date`, `as_of` | , today | The 90-day screening window. |
| `preferred_language` | | Interpreter reminder; agency checklist translation from `/data/templates/<language>/newcomer_checklist.md`. |
| `write_checklist` | false | Blank screening checklist (DOCX). |

CDC domestic refugee screening components with age rules (TST under 2, IGRA 2 and older; lead for
16 and younger with a repeat at 3-6 months for 6 and younger; HIV 13-64; hepatitis B for all and C
for adults), DPH and ORI programs (RHAP, RHPP, Refugee Medical Assistance), MassHealth coverage by
immigration status including the October 1, 2026 federal changes (EOM 26-10: adult refugees,
asylees, and several other statuses move to MassHealth Limited/Health Safety Net or Family
Assistance; children under 21 and pregnant or postpartum people are protected), and Emergency
Assistance family shelter and basic-needs resources.

<!-- End of Phase 9 tools. -->

## Phase 10: Healthy aging and mental health screening

<!-- Phase 10 tools go below this line. -->

All Phase 10 reference content is in `content/bh/` and `content/aging/`, drafted on 2026-11-08
and 2026-11-09 from the cited sources, `status: pending` (review due 2027-02-01). Town
directories are dated snapshots built by `cargo xtask build-service-directories`; nursing home
data by `cargo xtask build-nursing-home-snapshot`; older-adult ACS tables by
`cargo xtask build-older-adult-snapshots`.

### `score_mental_health_screen` (MH-02, toolset `bh`)

Scores a free screening instrument from its item responses: PHQ-2, PHQ-9, GAD-2, GAD-7,
Edinburgh Postnatal Depression Scale (EPDS), NIMH Ask Suicide-Screening Questions (ASQ), or the
C-SSRS screener. It never diagnoses.

| Argument | Default | |
|---|---|---|
| `instrument` | none (lists instruments) | `phq2`, `phq9`, `gad2`, `gad7`, `epds`, `asq`, `cssrs_screener` (aliases such as "PHQ-9" work). |
| `responses` | | Item answers in order: numbers, `true`/`false`, or text ("yes", "several days", "skipped"). |
| `epds_entered_as` | `scores` | `option_positions` to enter the position of the chosen answer; the tool reverse-scores items 3 and 5-10. |
| `age_years`, `refused` | | ASQ: a youth's (under 18) refusal is a non-acute positive screen; an adult's is not. |
| `asq_attempt_within_12_months`, `asq_bssa_documented_since_attempt` | | ASQ item 4 follow-up (the past-attempt exception). |
| `cssrs_behavior_past_3_months` | treated as yes when unknown | C-SSRS item 6 follow-up. |
| `difficulty` | | PHQ-9 functional difficulty (recorded, not scored). |
| `file`, `sheet` | | A CSV/XLSX of screens in `/data` (columns `q1`..`qN`); per-row results go to `/data/output`, rows with alerts first. |

Returns the total, severity band, outcome (`negative`, `positive`, `acute_positive`,
`non_acute_positive`, `positive_acuity_unknown`, `no_risk`/`low_risk`/`moderate_risk`/`high_risk`,
`incomplete`), each item, missing items, the published cutoff with its citation, and **safety
alerts** with cited steps: PHQ-9 item 9 or EPDS item 10 above zero (same day, whatever the
total), ASQ acute (immediate) and non-acute positive (brief suicide safety assessment, safety
plan, follow-up within 72 hours), and C-SSRS high, moderate, or low risk. 988 and the Crisis
Text Line are given to everyone screened for suicide risk. Item wording is bundled only for the
public-domain NIMH ASQ; PHQ/GAD, EPDS, and C-SSRS items are listed by topic and their license
terms are recorded. File mode returns suppressed counts by outcome and the number of rows with a
safety alert. Golden tests: `crates/phn-tools/tests/screening_golden.rs`.

Unanswered safety items are not treated as "no": when PHQ-9 item 9 or EPDS item 10 is missing,
`safety_items_unanswered` lists it and the answer says to ask it before the person leaves. A
C-SSRS screener with a yes answer but unanswered items that decide the risk level (item 6, or
items 3–5 unless item 2 was answered no) is "risk level not yet determined", with at least the same-day
(moderate risk) steps until the items are asked. In file mode, rows whose suicide-risk item is
missing or unreadable are listed first too; `rows_scored` and `rows_incomplete` are null when any
outcome count is suppressed, and a number of rows with an alert below the threshold is hidden.

### `find_crisis_resources` (MH-01, toolset `bh`)

| Argument | Default | |
|---|---|---|
| `town` | configured town | Massachusetts city or town. |
| `audience` | `all` | `youth`, `adult`, or `all`. |
| `language` | | A preferred language; the answer says how to reach each line in it. |

Lists 911 (immediate danger), the 988 Suicide and Crisis Lifeline (call, text, chat; Spanish,
interpretation in 240+ languages, ASL videophone, TTY, veterans), the Massachusetts Behavioral
Health Help Line (833-773-2445, call or text, 200+ languages), Mobile Crisis Intervention for
youth and adults (877-382-1609), the Crisis Text Line, and the town's Community Behavioral
Health Center(s) from mass.gov's listing (`cbhc_match`: `listed`, or `same_county` when no
published "Serving" list names the town). Says which line to use when. No warmline is listed
because none could be verified.

### `steadi_fall_risk_assessment` (AGE-01, toolset `aging`)

| Argument | |
|---|---|
| `stay_independent` | The 12 Stay Independent answers (yes = true). At risk at 4 points or more, or any fall in the past year. |
| `unsteady`, `worries_about_falling`, `fell_past_year` | The three key questions (alternative screen). |
| `falls_count`, `fall_injury` | Fall history. |
| `age_years`, `sex` | For CDC's 30-Second Chair Stand below-average scores (ages 60-94, men and women). |
| `tug_seconds`, `tug_observations` | Timed Up and Go (12 seconds or more = at risk) and gait observations. |
| `chair_stands`, `chair_used_arms` | 30-Second Chair Stand (using arms scores 0). |
| `balance_seconds` | 4-Stage Balance Test times in order (tandem under 10 seconds = increased risk). |
| `orthostatic` | Lying and standing blood pressures and symptoms (drop of 20 systolic or 10 diastolic, or symptoms = abnormal). |
| `other_risk_factors`, `home_hazards` | Medications, vision, feet, vitamin D, ...; hazards matched to CDC's Check for Safety fixes (`all` for the whole checklist). |
| `plan_format` | `docx` or `pdf`: a fall prevention plan in `/data/output` (no names). |

Returns the screen, each test against CDC's threshold, the risk factors with their evidence, the
STEADI intervention plan by factor, assessments still to do, prevention (when not at risk),
follow-up (30-90 days when at risk), home-safety fixes, and CDC's test procedures. A
`falls_count` of 1 or more counts as a fall in the past year (at risk); `fell_past_year: false`
with a count of 1 or more is an error to correct.

### `elder_protective_services_guide` (AGE-03, toolset `aging`)

| Argument | |
|---|---|
| `age_years`, `has_disability`, `setting` | Routes to Adult Protective Services (60+, community; 800-922-2275), DPPC (18-59 with a disability; 800-426-9009), or DPH (abuse by nursing home or hospital staff; 800-462-5540). |
| `concern` | Physical, sexual, emotional, caregiver neglect, financial exploitation, or self-neglect. |
| `emergency`, `reporter_role` | Emergency steps; whether the role is a mandated reporter. |
| `details_file`, `sheet`, `format` | Template + local fill: one row per report in `/data`; narratives go to `/data/output` only. |

Returns the pathways, mandated reporter lists (M.G.L. c.19A s.15, c.19C s.1), duties (verbal
report immediately, written within 48 hours), what a report contains (s.15(e)), timelines
(review within 48 hours, emergency assessment within 24 hours, investigation within 30 days,
written notice to the reporter within 45 days), definitions, signs by type, self-neglect,
protections and penalties, and the report-narrative template with its column names. Identifying
details are never accepted as arguments. `mandated_reporter` compares the role with the statutory
list for the chosen pathway only (c.19A for adults 60 and older, c.19C for adults 18–59 with a
disability): `true` when it matches an entry (a word match, not a legal determination), and null
when no role is given, when the role is not found on that list (the answer then says to read the
list against the statute), or for the DPH facility pathway.

### `find_ma_aging_services` (AGE-02, toolset `aging`)

| Argument | |
|---|---|
| `town` | Default: the configured town. |
| `needs` | Any of `home_care`, `meals`, `insurance`, `caregiver`, `dementia`, `protective`, `long_term_care`, `community`, `information` (default all). |

Returns the town's Aging Services Access Point (Boston: three, by neighborhood), Council on Aging
(how to reach it; contacts are not bundled), SHINE office and phone, senior nutrition project,
statewide lines (MassOptions, MassRelay, elder abuse hotline, Long-Term Care Ombudsman,
Alzheimer's Association helpline, SHINE for Chinese speakers), and program descriptions.

### `explain_older_adult_coverage_options` (AGE-04, toolset `aging`)

All arguments are optional: `age_years`, `household_size`, `monthly_income`,
`countable_assets`, `has_medicare_a_and_b`, `has_masshealth_standard`, `has_disability`,
`nursing_home_level_of_care`, `living` (`community`, `assisted_living`, `nursing_home`),
`has_private_insurance`, `town`.

Returns Medicare 2026 premiums, deductibles, coinsurance, and income-related Part B premiums
(CMS), the Part D out-of-pocket threshold, MassHealth's FPL figures (effective March 1, 2026),
and for each program (Medicare Savings Programs, MassHealth 65+, Frail Elder Waiver, PACE, SCO,
One Care) its rules and a screen (`likely_eligible`, `may_be_eligible`, `probably_not_eligible`,
`need_more_information`) with reasons, the MSP level (QMB at or below 190% FPL, SLMB/QI at or
below 225%), and PACE availability in the town's county. Not an eligibility decision.

### `nursing_home_quality_lookup` (AGE-06, toolset `aging`)

| Argument | Default | |
|---|---|---|
| `name`, `ccn`, `town`, `county` | configured town | Filters (names are fuzzy-matched). |
| `statewide` | false | Statewide summary. |
| `sort_by` | `name` | `overall_rating`, `staffing`, `infection_control`, `staff_flu_vaccination`. |
| `limit` | 25 | |
| `include_citations` | when 5 or fewer | Infection control citations (three years). |

Live from the CMS Provider Data Catalog (Provider Information 4pq5-n9py, Health Deficiencies
r5ix-sfxw, SNF QRP fykj-qjee, MDS Quality Measures djen-97ju, State/US Averages xcdc-v8bm),
cached for a day, with the `cms-ma-nursing-homes` snapshot offline. Returns star ratings,
staffing hours and turnover, inspections, fines and penalties, Special Focus and abuse flags,
infection control F-tags with scope and severity, staff and resident COVID-19, flu, and
pneumococcal vaccination, and Massachusetts and U.S. averages. CMS lists a facility's postal city,
which can be a village, neighborhood, or variant spelling ("Hyannis", "Dorchester",
"Middleboro"); such facilities are matched to a town by ZIP code, shown with the postal name in
parentheses, and a caveat says a ZIP code can cross a town line.

### `map_older_adult_vulnerability` (AGE-05, toolset `aging`)

| Argument | Default | |
|---|---|---|
| `area` | configured town | City or town, "X County", tract GEOID, or "Massachusetts". |
| `subareas` | tracts for a town; towns otherwise | `tract` or `town`. |
| `rank_by` | `composite` | `living_alone`, `disability`, `no_vehicle`, `limited_english`, `poverty`. |
| `limit` | 15 | Sub-areas listed (the map shows all). |
| `map` | false | Choropleth SVG and PNG in `/data/output` (via `create_choropleth_map`). |

Returns the area's five indicators with 90% margins of error, counts, reliability flags (CV over
30%), and Massachusetts and U.S. values; the age profile (Phase 4 ACS adapter, live with a
Census key); and sub-areas ranked by the chosen indicator or by how many of the five exceed the
state value. ACS 2020-2024 tables B09020, B25045, B16004, B17001, and C18108.

### `advance_care_planning_toolkit` (AGE-07, toolset `aging`)

| Argument | Default | |
|---|---|---|
| `session_minutes` | 60 | Scales the facilitator script (20-240). |
| `materials_format` | none | `docx` or `pdf`: facilitator script, handout, and blank proxy in `/data/output`. |
| `details_file`, `sheet`, `format` | | Template + local fill of health care proxies, one row per person, with an optional `Language` column. |

Returns health care proxy rules from M.G.L. c.201D (witnesses, who can be an agent, when
authority starts, revocation), the proxy template and its fields, what to do after signing, MOLST
status (not verified; the old molst-ma.org domain is no longer the program's site), a link to The
Conversation Project, the session script, and the handout. Agency translations of the proxy go in
`/data/templates/<language>/health_care_proxy.md`; rows in a language without one use English,
and the result counts them by language.

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
Syllable counting handles silent *e* inside words and compounds (safety, careful, caregiver) and
common public health words (diarrhea, diabetes, influenza, reaction), so grades can differ
slightly from version 0.14.0.
Golden tests: CDC *Simply Put*'s SMOG table and hepatitis A Fry example, and the score sheet's
arithmetic (`crates/phn-tools/tests/readability_golden.rs`).
### `get_overdose_data` (MH-03, toolset `bh`)

| Argument | Default | |
|---|---|---|
| `area` | configured town, else Massachusetts | A city or town, a county, or `Massachusetts`. |
| `compare_with` | | Up to 5 other towns or counties. |
| `basis` | `residence` | Town counts by `residence` (DPH Table 1) or `occurrence` (Table 2). |
| `drug` | `all` | State and U.S. drug class: `all`, `opioids`, `synthetic_opioids` (fentanyl), `heroin`, `prescription_opioids`, `methadone`, `cocaine`, `stimulants`. |
| `months` | 13 | 12 month-ending CDC periods to list (max 60). |

Towns: DPH's *Number of Opioid-Related Overdose Deaths, All Intents by City/Town, 2015-2022*
(posted June 2023, the latest town table DPH published as a document; bundled snapshot), with a
2018-2022 average annual rate per 100,000 (ACS 2020-2024 population, exact Poisson interval),
compared with the state. No rate is given for fewer than 5 deaths (DPH's rule in its 2024
report); rates on fewer than 20 deaths are flagged unstable. Counties: CDC/NCHS provisional 12
month-ending drug overdose deaths by county of residence (`gb4e-yj24`; counts of 1-9 suppressed
by NCHS). State and U.S.: CDC/NCHS provisional counts by drug class (`xkb8-kh2a`, by state of
occurrence), with the change from a year earlier. Both CDC datasets are live → cache → snapshot.
Adds DPH's statewide findings for 2024 (March 2026 report, with pages) and a link to the BSAS
dashboard for newer town data.

### `get_youth_risk_behavior_data` (MH-07, toolset `bh`)

| Argument | Default | |
|---|---|---|
| `topic` | mental health and suicide | Words such as `sad or hopeless`, `suicide`, `self-harm`, `bullied`, `vaping`, `alcohol`, `marijuana`, `belong at school`. |
| `school_level` | `high` | `high`, `middle`, or `both`. |
| `by` | all | Any of `sex`, `grade`, `race_ethnicity`, `lgbtq`. |
| `survey` | both | `MYHS` or `MYRBS`. |
| `compare_us` | true | Add CDC YRBS mental health estimates for Massachusetts and the U.S. |
| `max_indicators` | 8 | Max 25. |

Massachusetts indicators come from DPH's report tables (bundled): the Massachusetts Youth Health
Survey 2025 (April 2026) and 2023, and the 2023 joint MYHS/YRBS report (305 indicators, 3,565
estimates with 95% confidence intervals; `**` estimates DPH did not present stay suppressed).
Groups are compared with male, White, and straight/cisgender students, and a difference is shown
only when the 95% intervals do not overlap (DPH's rule); the same indicator two years earlier is
compared the same way. CDC's YRBSS mental health indicators (`nu3s-3dwd`, 2019-2023, live → cache →
snapshot) compare Massachusetts with the national YRBS. Suicide-related answers include DPH's 988
Lifeline text and a safe-messaging reminder.

### `find_naloxone_and_harm_reduction` (MH-04, toolset `bh`)

| Argument | Default | |
|---|---|---|
| `town` | configured town | For syringe service program approval and approved towns in the same county. |
| `include_teaching_script` | true | |
| `handout` | none | `docx`: an accessible Word handout of the script, access options, and help lines in `/data/output`. |

From `content/bh/substance_use_resources.yaml`: ways to get naloxone (pharmacy counter under the
statewide standing order with insurance including MassHealth and no MassHealth co-pay; BSAS
distribution programs; home delivery through You Can Save a Life; agency purchasing), the
standing order's five preferred formulations and directions (signed July 13, 2026), pharmacy
duties (Board of Pharmacy Policy 2024-01), legal protections (c. 94C §§ 19B, 34A), whether the
town's board of health approved a syringe services program (DPH's list; c. 111 § 215), fentanyl
test strip facts (CDC; the legal status is marked unverified), overdose risk factors, contacts
(Substance Use Helpline, Never Use Alone line, 988), and an 8-step teaching script citing DPH's
OEND core competencies, the standing order, and CDC. DPH's site maps cannot be read, so they are
linked. Decision support.

### `find_substance_use_treatment` (MH-05, toolset `bh`)

| Argument | Default | |
|---|---|---|
| `town` | configured town | Search center (the town's label point, converted from MassGIS state plane to latitude/longitude). |
| `radius_miles` | 15 | 1 to 75. |
| `services` | | All must match: `methadone` (opioid treatment program), `buprenorphine`, `naltrexone`, `withdrawal_management`, `residential`, `outpatient`, `intensive_outpatient`, `recovery_support`, `sober_housing`, `adolescents`, `pregnant_postpartum`, `co_occurring`, `veterans`. |
| `payment` | | `masshealth` (Medicaid), `medicare`, `private`, `self_pay`, `sliding_fee`. |
| `language` | | `spanish`, `asl`, or another language (`portuguese`, `creole`, `vietnamese`, ...). |
| `max_results` | 15 | Max 50. |

SAMHSA's FindTreatment.gov locator API (`phn_data::findtreatment`; distance search, substance
use facilities) through live → cache → a bundled statewide copy (`samhsa-findtreatment-ma`, 341
facilities; distances computed from the town center when it is used). Returns each facility's
address, phones, website, miles, medications for opioid use disorder, settings, payment, and
languages, plus the Massachusetts Substance Use Helpline, 988, and FDA's MOUD categories.
Payment and language filters match SAMHSA's values exactly, so "Medicaid" does not match "State-
financed health insurance plan other than Medicaid" and "Spanish" does not match "Other languages
(excluding Spanish)"; buprenorphine does not match a facility that uses it only for pain
management or emergency dosing. Facilities outside Massachusetts (by ZIP code) are flagged: their
"Medicaid" is that state's program, not MassHealth.

### `plan_school_postvention` (MH-06, toolset `bh`)

| Argument | Default | |
|---|---|---|
| `cause_status` | `unconfirmed` | `unconfirmed`, `ruled_suicide`, or `family_declined_disclosure`; chooses the student and parent templates. |
| `phase` | `all` | `first_24_hours`, `first_week`, `ongoing`. |
| `messages` | staff meeting, student notification, parent letter | Also `media_statement`, `social_media_post`. |
| `draft_text` or `draft_path` | | A draft to check against the safe-messaging rules. |
| `details_path` | | CSV/XLSX in `/data` whose columns match the placeholders; each row fills the templates as Word files in `/data/output`. |

Steps (16, by phase, with page numbers) and talking points paraphrase AFSP/SPRC *After a
Suicide: A Toolkit for Schools* (2nd ed., 2018). The templates are this project's own drafts that
follow the toolkit's structure (the toolkit allows copying with credit, but adapting or
excerpting needs EDC's permission; the license is recorded in the content), use 988, and pass the
safe-messaging rules. The safe-messaging check (`content/comms/safe_messaging.yaml`, shared with
the communication tools) flags method or scene details, outdated language, single causes,
glamorizing, sensational words, PA or assembly announcements, and memorials on school grounds,
and missing help resources or hope; findings cite sentence numbers and never repeat method
words. Local fill follows the template + local fill rule: names are never arguments and never
returned. Decision support.

### `translate_health_material` (COMM-02, toolset `comms`)

| Argument | Default | |
|---|---|---|
| `source_text` or `source_path` | | The English source (excerpts from a file are redacted). |
| `language` | | Target language (31 recognized, e.g. Spanish, Portuguese, Haitian Creole, Cape Verdean Creole, Vietnamese, Chinese (Simplified/Traditional), Arabic, Khmer). |
| `glossary_path` | | Agency glossary CSV/XLSX in `/data`: `english` plus one column per language (name or code), or `english`, `language`, `translation`. |
| `translation_text` or `translation_path` | | A translated draft to check. |
| `back_translation_text` or `back_translation_path` | | An English back-translation by someone other than the translator. |
| `town` | configured town | For language priorities (EQ-03, ACS limited English proficiency). |
| `material_name` | | File stem to look for in `/data/templates/<language>/`. |
| `worksheet` | none | `xlsx`: a review worksheet (segments, draft, back-translation, comments, approval) in `/data/output`. |

No translations are bundled (product-owner decision). The tool reports agency-approved
translations found in `/data/templates/<language>/` (expected layout: one folder per language,
named by language or code, holding files named like the English material), builds a translator
brief (numbered segments; approved glossary terms in the source; numbers, phone numbers, times,
web and email addresses that must carry over), checks a draft for missing glossary terms (as whole
words with a plural ending; anywhere in scripts written without spaces, such as Chinese) and
changed numbers or phones (decimals, thousands separators, and times such as 9:30 are read whole,
and local formats such as 1.000 or 3,5 match by their digits), and compares a back-translation
with the source (sentence count and
coverage, negations, glossary terms). Every result carries the label "DRAFT TRANSLATION - for
review by a qualified translator" and CDC *Simply Put*'s translation steps and pitfalls. Script
notes point to DOCX for scripts the PDF writer cannot render (ADR 0003).

### `create_social_media_campaign` (COMM-03, toolset `comms`)

| Argument | Default | |
|---|---|---|
| `topic` | required | e.g. `flu vaccine`, `naloxone`, `lead poisoning`. |
| `platforms` | facebook, instagram, x | Also `linkedin`, `bluesky`, `threads`. |
| `start_date`, `weeks`, `posts_per_week` | today, 4, 2 | Max 26 weeks and 7 posts a week. |
| `posts` | | Draft posts to check: `platform`, `text`, `alt_text`, `has_image`, `date`. |
| `observances_path` | | Your own observance calendar (CSV/XLSX: `name`, `start`, `end`). |
| `char_limits` | | Override platform limits, e.g. `{"x": 280}`. |
| `target_grade` | 8 | |
| `export` | none | `xlsx` calendar with posts and issues in `/data/output`. |

Plans posting slots (Tuesday and Thursday first) and ties them to related observances (20 bundled
observances with date rules, marked unverified because ODPHP's National Health Observances pages
were unavailable; or the agency's own calendar). Checks drafts for platform length (links on X
counted as 23 characters), alt text (WCAG 1.1.1; no "image of"), CamelCase hashtags, hashtag
count, emoji, words in capitals, reading grade and jargon (COMM-01 engine), and safe messaging when
the post is about suicide. Platform limits could not be verified from this environment and are
overridable.

### `draft_press_materials` (COMM-04, toolset `comms`)

| Argument | Default | |
|---|---|---|
| `kind` | `press_release` | `media_advisory`, `talking_points`, `qa`. |
| `headline`, `facts` (`who`, `what`, `when`, `where`, `why`, `how`) | | |
| `key_messages`, `call_to_action`, `quotes` (`speaker`, `text`) | | Quote officials only. |
| `contact` (`name`, `phone`, `email`, `after_hours_phone`) | | |
| `release_date`, `embargo_until` | today, none | No embargo gives "For immediate release". |
| `target_grade` | 8 | Reading grade to aim for. |
| `draft_text` | | Check an existing draft instead of the built structure. |
| `export` | none | `docx` in `/data/output`. |

Builds the structure from the facts with `[BRACKETED]` gaps listed, then checks it (or a draft)
against CDC's CERC *Working with the Media* (2018): contact block, date, release time or embargo,
5 Ws, headline, one page, sentences of 20 words or fewer, no loaded words, and a privacy check for
client identifiers; adds the reading grade, safe messaging for suicide topics, CERC's six
principles, and the CERC wallet card's questions for a spokesperson Q&A. Loaded words match whole
words or word starts ("panic", "panicked"), never the inside of another word ("Hispanic").

<!-- End of Phase 11 tools. -->

## Phase 12: Emergency preparedness I: situational awareness and planning

<!-- Phase 12 tools go below this line. -->

### `get_active_hazard_alerts` (EPR-05, toolset `epr`)

| Argument | Default | |
|---|---|---|
| `area` | saved town, else Massachusetts | Town, county ("Bristol County"), or "Massachusetts". |
| `guidance_for` | | Hazards to plan for without an active alert (`extreme_heat`, "winter storm", "Hurricane Warning", ...). |
| `include_marine` | false | List alerts for coastal waters only. |
| `include_declarations` | true | FEMA disaster declarations. |
| `declaration_years` | 10 | Years of declarations listed (1–75). |

Live NWS alerts (`api.weather.gov/alerts/active?area=MA`, cached five minutes, never from a
snapshot) are filtered to the area by county FIPS from each alert's SAME codes; cancelled, test,
and marine-only alerts are left out. Each NWS event maps to a hazard in
`content/epr/hazard_health_actions.yaml` with cited health effects, at-risk groups, public
health actions, and public messages (CDC topic pages, MEMA safety tips, CDC PHEP capabilities),
plus the follow-on power outage and carbon monoxide risks. FEMA declarations come from OpenFEMA
(live, cached a day, bundled snapshot fallback), grouped by declaration, with counts by incident
type since 1953. When alerts cannot be reached the first caveat says so in capitals. A cached copy
of the alert feed is used only when it is no more than 60 minutes old; otherwise the alerts are
reported as not retrieved. Alerts that have ended (or expired), test alerts, and cancellations
are left out.

### `model_dispensing_site_throughput` (EPR-02, toolset `epr`)

| Argument | Default | |
|---|---|---|
| `population` | required | People who need the countermeasure. |
| `model` | `oral_prophylaxis` | Or `vaccination`. |
| `sites` | 1 | Sites open at once. |
| `operating_hours` or `arrivals_per_hour` | one required | Hours each site is open to reach everyone, or arrivals per site per hour. |
| `regimens_per_visit` | 1 | Head-of-household pickup. |
| `stations` | model defaults | Overrides per station: `minutes_per_client`, `fraction_of_clients`, `staff` (to evaluate a plan), or new stations. |
| `max_utilization`, `target_wait_minutes` | 0.85, 10 | Sizing targets (tool defaults). |
| `hours_open_per_day`, `shift_hours`, `support_staff_per_shift`, `surge_factor` | 12, 6, 0, 1.25 | Calendar, shifts, and surge check (tool defaults). |
| `worksheet` | false | Staffing XLSX in `/data/output`. |

Each station is an M/M/c queue (`phn_epi::queueing`): arrivals λ × share of clients, service
rate 60 ÷ minutes, Erlang C probability of waiting through the Erlang B recursion, mean wait
Wq = C/(cμ − λ), line length, P(wait > target), and capacity. Sizing picks the fewest staff with
utilization ≤ the maximum and Wq ≤ the target. Site capacity is the bottleneck station's;
hours needed = visits ÷ (sites × served per hour); people inside come from Little's law.
Golden tests: exact rational values and the County Hospital example in Hillier and Lieberman.
Definitions and activation elements are from CDC PHEP Capability 8; station times are tool
defaults (the AHRQ and CDC planning guides with published times could not be retrieved).

### `air_quality_protective_guidance` (ENV-02, toolset `env`)

| Argument | Default | |
|---|---|---|
| `area` | saved town, else Massachusetts | Town (nearest reporting area), county (each town's nearest area), or the state (all 11 areas). |
| `aqi`, `pollutant` | | Guidance for an AQI you already have. |
| `concentration`, `pollutant`, `ozone_averaging` | | Convert a concentration to an AQI. |

Current NowCast, yesterday's, and forecast AQI per pollutant come from AirNow's public
reporting-area file (no key; Massachusetts forecasts by MassDEP); with `PHN_AIRNOW_API_KEY` the
AirNow API is tried first (untested: the host was unreachable when built). Guidance is for the
worst category among the current reading and the forecasts for today and tomorrow: EPA levels
of health concern, pollutant-specific sensitive groups, and actions for sensitive groups, the
public, schools (EPA-456/F-14-003, transcribed), and outdoor events (summaries, medium
confidence). AQI conversion uses 40 CFR 58 Appendix G (2024 PM2.5 breakpoints), truncation, and
Equation 1; tests reproduce the appendix's worked example. A concentration above a table that
stops below AQI 500 (8-hour ozone above 0.200 ppm, 1-hour SO2 above 304 ppb) is an error that names
the averaging time Appendix G uses for higher values; only tables that reach AQI 500 are
extrapolated. Offline, every category's actions are listed.

### `heat_health_action_planner` (ENV-01, toolset `env`)

| Argument | Default | |
|---|---|---|
| `area` | saved town | Town or county. |
| `plan_for_level` | forecast peak | HeatRisk level 0–4 to plan for. |
| `cooling_centers_path` | | CSV/XLSX in `/data` (name, address, town, hours, phone, accessible, pets, notes); rows go to the checklist only. |
| `checklist` | true | DOCX checklist in `/data/output`. |

The 7-day NWS HeatRisk is read from WPC's KML ground-overlay PNGs (palette index = level) and
sampled over the town or county boundaries (EPSG:26986 ↔ latitude/longitude, point in polygon);
the NWS point forecast is for the town (or the county's most populous town); heat alerts come
from the NWS API. The heat-vulnerable population combines ACS (65+, 85+, under 5, disability,
poverty, no vehicle, limited English, no internet, renters), CDC PLACES adult prevalence (town
values are tract-weighted), SVI tracts in the top quarter, and HHS emPOWER. Activation steps are
cumulative to the level; the checklist adds outreach scripts, schools and child care, outdoor
workers (NIOSH), heat illness signs and response (MEMA), and medications and power outages (CDC).
`heat_alerts_status` says whether the NWS alert feed was read live, from the cache, or not at
all; when it could not be read, the answer and the checklist say "NWS alerts could not be checked
— see weather.gov" instead of "none". Expired and test alerts are left out. The peak HeatRisk day
is the first day the highest level is reached. A HeatRisk image saved before WPC's latest update
is not paired with the new dates; a caveat gives the reason when a cached copy was used.

### `hazard_vulnerability_assessment` (EPR-06, toolset `epr`)

| Argument | Default | |
|---|---|---|
| `area` | saved town, else Massachusetts | Town or county. |
| `scores` | | Per hazard: `probability` (0–3), `human`, `property`, `operations` (0–3), `preparedness`, `internal_response`, `external_response` (1–3, 3 = weak). |
| `use_seeds` | true | Fill missing probability and human impact from ResilientMass ratings. |
| `nri_path` | | A FEMA National Risk Index table downloaded into `/data`. |
| `workbook` | true | XLSX facilitation workbook in `/data/output`. |

Relative risk = (probability ÷ 3) × (sum of the six severity items ÷ 18) × 100, ranked. Seeds:
the 2023 ResilientMass Plan's statewide likelihood, human consequence, warning time, and scale
for its 15 hazards (Tables 5.1-1 to 5.1-5), FEMA declarations for the county since 1953, and NRI
scores from the nurse's file (prefix + `_RISKS`, `_RISKR`, `_AFREQ`). Six local hazards are added
(pandemic, power outage, water, hazmat, mass casualty, cyber). The scoring method is the tool's
own, modeled on common HVA worksheets. The NRI could not be bundled: hazards.fema.gov/nri now
redirects to FEMA's Resilience Analysis and Planning Tool, which was blocked.

### `functional_needs_shelter_intake` (EPR-03, toolset `epr`)

| Argument | Default | |
|---|---|---|
| `area` | saved town, else Massachusetts | Local rates for the estimates. |
| `expected_residents` | | Planning estimates of needs and staff. |
| `blank_form` | `docx` | `xlsx`, `both`, or `none`. |
| `intake_path` | | CSV/XLSX of registrants in `/data` (question ids as columns). |
| `residents_per_health_staff`, `fnss_residents_per_support_staff`, `lep_residents_per_interpreter` | 50, 10, 25 | Staffing ratios (tool defaults). |

Returns the CMIST intake template (25 questions in five domains, dispositions) and the CDC PHEP
mass care tasks. Estimates apply local rates (ACS limited English, disability, 65+, under 5, no
vehicle; PLACES hearing, vision, cognition, mobility, self-care, independent living, COPD,
diabetes; emPOWER electricity dependence) to the expected residents. With `intake_path` it
writes one completed form per registrant to `/data/output` (in an agency translation from
`/data/templates/<language>/cmist_intake.md` when present, otherwise English, counted) and
returns only counts of "yes" by question and registrants by language, suppressed below the
threshold. The `<language>` folder is the language as written in the registration file, in lower
case, with spaces as underscores and other punctuation except `-` removed: "Haitian Creole" →
`haitian_creole`, "Spanish" → `spanish`. Codes and names are not matched to each other (a file
that says "es" needs a folder named `es`). Identifying details are never returned.

### `get_electricity_dependent_population` (EPR-04, toolset `epr`)

| Argument | Default | |
|---|---|---|
| `area` | saved town, else Massachusetts | Town, county, or the state. |
| `zip_codes` | | Specific ZIP codes instead. |
| `local_path` | | A ZIP-level aggregate file in `/data` (for example DPH's emPOWER Emergency Planning Dataset). |

HHS emPOWER counts (Medicare beneficiaries with claims for electricity-dependent equipment) are
read live from the emPOWER Map's JSON endpoints (the ArcGIS REST service is not needed), cached a
week, with a bundled snapshot. Counties are HHS's totals; towns allocate ZIP codes by the ZCTA's
land share in the town. Rates per 1,000 beneficiaries are compared with Massachusetts and the
U.S. Values of 1–11 are published as 11 and flagged. Local files with direct identifiers are
refused (the restricted outreach dataset must stay in secure agency systems); totals from
aggregate files are suppressed below 11 (CMS cell-size policy) or the agency threshold.

### `plan_casper_assessment` (EPR-11, toolset `epr`)

| Argument | Default | |
|---|---|---|
| `area` or `towns` | saved town | The sampling frame (a town, several towns, or a county). |
| `clusters`, `interviews_per_cluster` | 30, 7 | Other values make a modified CASPER. |
| `household_measure` | `total` | Or `occupied`. |
| `seed` | chosen and reported | Reproducible selection. |
| `exclude_blocks_under` | | Leave out small blocks (modified frame). |
| `materials` | true | Cluster list and tracking forms (XLSX) and team packet (DOCX) in `/data/output`. |
| `results_path`, `cluster_column`, `times_selected_column`, `questions`, `frame_households`, `tracking` | | Weighted analysis of returned data. |

Stage I follows the CASPER Toolkit (3rd ed., section 2.4): 30 random household numbers between 1
and the frame total (SplitMix64 with rejection sampling) mapped to 2020 census blocks by
cumulative households (PL 94-171 snapshot: 88,246 blocks with housing); blocks can be chosen
more than once. Analysis uses the toolkit weight N ÷ (interviews in the cluster ÷ times selected
× clusters selected), weighted proportions with Taylor-series variance (clusters as PSUs, t with
clusters − 1 df; golden test against `samplics`), design effects, contact, cooperation, and
completion rates, the 80% completion rule (otherwise unweighted), and no estimates for cells
under the larger of 5 and the agency's suppression threshold. When exactly one answer to a
question is that small, the next smallest answer is withheld too, so the total cannot reveal it.
The stage II sampling interval (every *n*th household) is households ÷ (interviews × times the
block was selected), rounded down. An analysis of returned data without the plan's
seed draws no new sample and writes no new materials. A cluster selected more than once counts
as one sampling unit for the confidence intervals; a caveat explains how to match the toolkit.

<!-- End of Phase 12 tools. -->

## Phase 13: Emergency preparedness II: ICS, plans, and exercises

Staff names and positions may be passed in a request; phone numbers, email addresses, and radio
call signs come only from a roster file in `/data` (`roster_path`) and go only into documents
written to `/data/output` (ADR 0009). When a name is given, a person's contact details are filled
in only when the roster has that exact full name (case and punctuation ignored); a name that is
not on the roster gets no contact details, never those of someone else in the same position. The
position (its first holder) is used only when no name is given (ADR 0010). A roster with no name
or position column is an error that lists the roster fields it did recognize, not the column
names. Every text field of a request is scanned for typed-in phone numbers and emails, except
file paths, dates and times, and fields where a facility's number belongs.

### `generate_ics_forms` (EPR-01, toolset `epr`)

| Argument | Default | |
|---|---|---|
| `forms` | `iap` (202, 203, 204, 205, 205A, 206, 207, 208) | Any of 201, 202, 203, 204, 205, 205A, 206, 207, 208, 213, 213RR, 214, 215A, or `all`. |
| `incident_name`, `incident_number` | | Block 1 (and 2) of every form. |
| `period_start`, `period_end` | | Operational period, `YYYY-MM-DD HH:MM` (24-hour clock). |
| `initiated` | `period_start` | ICS 201 date and time initiated. |
| `prepared_at`, `prepared_by`, `approved_by` | now; the Planning Section Chief; the Incident Commander | Prepared-by and approval blocks (signatures are by hand). |
| `objectives`, `command_emphasis`, `situational_awareness`, `situation_summary`, `actions`, `safety_message`, `site_safety_plan_required`, `site_safety_plan_location` | | ICS 201, 202, and 208 text. |
| `organization` | from the roster's position column when empty | Position, name, agency (names only; never phone numbers). |
| `assignments` | | One ICS 204 per Division/Group: supervisor, branch director, resources, work assignments, special instructions, communications. |
| `resources`, `radio_channels`, `radio_instructions`, `medical_plan`, `messages`, `resource_requests`, `activity_logs`, `safety_analysis`, `other_attachments` | | ICS 201 block 10, 205, 206, 213, 213RR, 214, 215A, and 202 block 6. Facility numbers (hospital, ambulance dispatch) may be given; never personal numbers. |
| `incident_type`, `use_suggestions` | none, `false` | `measles_outbreak`, `respiratory_outbreak`, `mass_dispensing`, `coastal_storm`, `extreme_heat`, `flooding`, `hazmat` (or words such as "POD", "hurricane"). Returns suggested SMART objectives, command emphasis, safety message, and ICS 215A lines; with `use_suggestions` they go into empty blocks marked "suggested; review". |
| `map_path`, `map_description` | | PNG/JPEG map in `/data` for ICS 201 block 4 and the IAP map page (with alt text). |
| `roster_path`, `roster_sheet` | | Staff roster CSV/XLSX in `/data` (name, position, agency, work and cell phone, email, radio). |
| `format` | `docx` | `pdf` or `both`. |
| `blank_forms` | `false` | Also write blank copies with FEMA's block instructions to `output/…/blank/`. |

Writes one file per form (one copy per Division/Group, message, request, or log) and, when two
or more IAP forms are requested, an assembled Incident Action Plan (cover, contents, forms in
IAP order, map page) to a new folder in `/data/output`. The forms keep FEMA's titles, block
numbers, and labels (NIMS ICS Forms Booklet, FEMA 502-2; forms v3 and v3.1, checked against
FEMA's PDFs) as accessible DOCX/PDF documents; they are labeled as adaptations, not FEMA's
fillable PDFs. Returns, per form, the files written and the required blocks still blank (with
FEMA's instruction for each) so the assistant can ask for them; an IAP checklist; the form
catalog; and suggestions for the incident type, with citations and confidence. **Privacy (ADR
0009):** staff phone numbers, emails, and radio call signs come only from `roster_path` and are
written only to the documents (the ICS 205A is marked "not for public release"); the response
contains counts (staff, matched, unmatched), never roster values. Contact details or client
identifiers typed into the request are reported by category in the privacy block. The output
folder name is a plain ASCII version of the incident name (a slash or accent cannot change the
folder). The ICS 207 shows Branch Directors with their Divisions and Groups under the Operations
Section Chief; positions with no place on the chart are listed as "Other assigned positions".
### `continuity_of_operations_planner` (EPR-07, toolset `epr`)

| Argument | Default | |
|---|---|---|
| `agency_name`, `town` | letterhead, saved town | Plan title and jurisdiction. |
| `functions` | 19 seeded board of health functions | Change a seeded function by `id` (tier 1-4, `rto`, `minimum_staff`, `lead_position`, `authority`, `dependencies`) or add one with `label` and `tier`. |
| `exclude_functions` | | Seeded function ids to leave out. |
| `succession` | template positions | Each key position with `successors` in order (positions or names; three or more recommended) and an optional `incumbent`. |
| `delegations` | five drafted | Add delegations (`authority`, `delegated_to`, `trigger`, `limits`, `legal_basis`). |
| `alternate_sites` | | `name`, `kind` (`alternate`, `telework`, `devolution`), `location`, `capacity`, `agreement`, `ready_within_hours`. |
| `essential_records`, `communications` | default lists | Added to the defaults. |
| `staff_count`, `absenteeism_percent` | —, 40 | Staffing check at the planning absenteeism rate. |
| `devolution_partner` | | Neighboring board of health, health district, or DPH. |
| `pandemic_annex` | true | Pandemic and infectious disease annex. |
| `plan_date` | today | Start of the dated test, training, and exercise schedule. |
| `roster_path`, `roster_sheet` | | Staff roster (CSV/XLSX in `/data`): names, positions, phones, emails, radio. |
| `output` | `both` | `docx` (plan), `xlsx` (workbook), `both`, or `none`. |

Organized by the ten FEMA continuity elements (essential functions, orders of succession,
delegations of authority, continuity facilities, continuity communications, essential records,
human capital, test/training/exercise, devolution, reconstitution) and four phases (readiness,
activation and relocation, continuity operations, reconstitution). Seeded functions carry their
authority (105 CMR 300.110 and 300.160; M.G.L. c. 111 sections 30, 31, 95, 104, 122, 127A;
c. 114 section 45; c. 30B section 8; 310 CMR 15.000), a priority tier with a recovery time
objective, and minimum staff (tiers and staff are tool defaults). Delegations include the board's
emergency agent (c. 111 section 30: acts for the board in an emergency and reports within two
days), burial permit agents, emergency procurement, and isolation and quarantine orders; town
counsel must confirm each. The human capital check applies the federal pandemic planning
assumption (up to 40% of staff absent for about 2 weeks at the peak; 2006 Implementation Plan)
to `staff_count` (staff at work round down without losing a whole person: 10 staff at 80% absence
is 2) and compares the staff left with the Tier 1 and Tier 1-2 minimums. When a successor position
is held by several people, each mention gets the next holder, so no one is their own successor or
listed twice. The result
lists each element as `provided`, `partial`, `drafted` (tool template to confirm), or `missing`.

Privacy: roster contacts (and names read from the roster) are filled only into the plan's
succession table and call-down annex and the workbook in `/data/output`; the response reports
counts only (`roster`). Phone numbers or emails typed into the request are reported in the
privacy block with a request to use the roster instead. Decision support only.
### `design_hseep_exercise` (EPR-08, toolset `epr`)

| Argument | Default | |
|---|---|---|
| `exercise_type` | `tabletop` | `seminar`, `workshop`, `tabletop`, `game`, `drill`, `functional`, or `full_scale` (HSEEP 2020, Tables 2.3-2.9). |
| `scenario` | | Developed: `measles_outbreak`, `coastal_storm_power_outage`, `extreme_heat`. Seeds: `mass_dispensing_anthrax`, `pandemic_influenza`, `cyberattack`, `water_contamination`, `mass_casualty`, `hazmat_release`. Words such as "measles" or "hurricane" also work. Required except for seminars and workshops. |
| `custom_scenario` | | Your own scenario (fictional); modules get placeholders for injects. |
| `objectives` | the scenario's suggested objectives | Each `{text, capabilities}`; capabilities are PHEP numbers or names and/or FEMA core capabilities. |
| `area`, `agency`, `sponsor`, `exercise_name` | saved town; letterhead agency or "<town> Board of Health" | Fill `{{town}}` and `{{agency}}` in the scenario. |
| `date`, `start_time`, `duration_hours`, `location`, `scope` | 09:00; by type (tabletop 3.5 h) | Schedule and planning timeline. |
| `capabilities` | | Extra capabilities to exercise beyond those named in the objectives. |
| `participants`, `organizations`, `point_of_contact` | | Positions, optional names, exercise roles (player, facilitator, evaluator, controller, simulator, observer, planner). |
| `roster_path`, `roster_sheet` | | Staff roster (name, position, phone, email); contacts go only into the files. |
| `documents` | `docx` | `pdf`, `both`, or `none`. |

Objectives get a SMART check (word patterns for specific, measurable, relevant, and time-bound;
achievable is left to the planning team) and are mapped to the 15 CDC PHEP capabilities (2018
standards) and FEMA's 32 core capabilities; FEMA capabilities are added from this project's
PHEP mapping (not an official crosswalk), and PHEP capabilities are suggested from the wording
when none are given. Developed scenarios have three modules with injects, key issues, and
discussion questions by functional group, keyed to PHEP capabilities; seed and custom scenarios
get module questions built from the capabilities' functions. The schedule splits the time across
modules with breaks, a hotwash, and closing; the planning timeline dates the C&O, Initial,
Midterm, MSEL (operations only), and Final Planning Meetings (weeks are tool defaults) and the
after-action steps. Files in `/data/output/exercise_<name>/`: Situation Manual and Facilitator
Guide (discussion-based) or Exercise Plan (operations-based) plus an MSEL workbook (functional
and full-scale), Exercise Evaluation Guides (one per objective and PHEP capability, with the
capability's functions as critical tasks and the P/S/M/U ratings key), a Participant Feedback
Form, and a sign-in sheet. Nothing read from the roster is returned; the result counts matches.
When the schedule needs more time than `duration_hours` (fixed parts and a minimum of 20 minutes
a module), a warning gives the duration needed; registration and the controller/evaluator debrief
(after the exercise) are outside that length. MSEL event numbers count within each module ("1-1",
"1-2", "2-1"). PHEP capabilities are suggested from whole words or word starts in the objective
("media" is not found in "immediately"). Dates must be YYYY-MM-DD between 1900 and 2200.

### `build_after_action_report` (EPR-09, toolset `epr`)

| Argument | Default | |
|---|---|---|
| `name`, `kind` | `exercise` | `incident` for a real response. |
| `exercise_type`, `start_date`, `end_date`, `area`, `agency`, `location`, `scope`, `hazard`, `scenario_summary`, `sponsor` | | Overview fields (no client details). |
| `objectives` | | Each `{text, capabilities, ratings: [{capability, rating: P/S/M/U}]}`. Capabilities may be numbers, names, the display form "PHEP 13: Public Health Surveillance and Epidemiological Investigation", or the capability objects `design_hseep_exercise` returns. A rating that names an unrecognized capability is an error. |
| `observations` | | Each `{kind: strength or area_for_improvement, objective, capability, statement, reference, analysis, root_cause, recommendation}`. |
| `corrective_actions` | | Each `{area_for_improvement, action, element (planning, organization, equipment, training, exercise), organization, poc_name, poc_position, start_date, completion_date}`. |
| `participants`, `organizations`, `point_of_contact` | | Counts by level or organization; the POC's phone and email come from the roster. |
| `feedback_path`, `feedback_sheet` | | Participant Feedback Forms (one row per form; rating columns named like the form's statements or `q1`-`q8`, values 1-5 or the scale labels). |
| `roster_path`, `roster_sheet` | | Contacts for the POC and corrective-action owners (files only). |
| `documents` | `docx` | `pdf`, `both`, or `none`; the XLSX improvement plan tracker is always written unless `none`. |

Follows the HSEEP 2020 AAR/IP template: overview, Table 1 (capability ratings), strengths and
areas for improvement (with reference and analysis) by objective and capability, the
Improvement Plan (numbered by capability), participants, and a feedback summary. Checks: every
area for improvement has a corrective action; each action has an element, organization, point
of contact, and start and completion dates in order (after the exercise); capabilities resolve;
each objective's PHEP capabilities are rated; areas for improvement state the gap rather than a
recommendation; no person names (non-attribution) or client identifiers or contact details in
the text. An area for improvement with a recommendation but no action gets a drafted action
marked for the After-Action Meeting. Feedback forms are totaled locally: identifier columns
(names, emails) are skipped, comments are counted but never read back, and each statement's
counts are suppressed (with a complement) below the threshold; means and percent agreeing are
hidden when fewer forms than the threshold answered. After-action dates (30, 45, 60 days) are
tool defaults. A corrective action's `element` given as a single letter must be unambiguous ("E"
could be equipment or exercise, so it is not guessed).
### `crisis_risk_communication_builder` (EPR-10, toolset `epr`)

| Argument | Default | |
|---|---|---|
| `hazard` | (lists the library) | Pre-scripted set: `measles_exposure`, `boil_water`, `extreme_heat`, `coastal_storm_power_outage`, `shelter_in_place`, `respiratory_pandemic`, `mass_dispensing_pod` (aliases such as "measles", "hurricane", "POD" work). |
| `hazard_description` | | Any other hazard; the map is built from your facts. |
| `phase` | `initial` | `preparation`, `initial`, `maintenance`, `resolution`, or `all` (CERC rhythm). |
| `details` | | Values for the set's `{{placeholders}}` (place, times, agent). Never client details. |
| `what_happened`, `known`, `unknown`, `being_done`, `public_actions`, `next_update`, `info_source`, `organization` | | The holding statement; missing parts stay `[BRACKETED]` and are listed. |
| `message_map`, `empathy`, `draft_text` | | Check your own map (up to 3 key messages) or a draft. |
| `audiences`, `spokesperson` | the set's audiences | Audience concerns (CERC Table 1) and spokesperson preparation. |
| `area` or `languages` | saved town | Language plan from ACS limited-English counts (offline from snapshots), plus saved preferred languages. |
| `target_grade` | 8 | Reading grade for the checks. |
| `export` | | `docx`: packet, English template, and per-language agency translations or translator briefs in `/data/output/cerc_<id>/`. |
| `roster_path`, `media_contact_position` | , `Public Information Officer` | Media contact's name, phone, and email go into the packet only (ADR 0009). |

Content: `content/epr/cerc.yaml` (pending review): the CERC rhythm and phase tasks (CDC CERC
manual chapters 2014–2019), PHEP Capability 4 functions, message-map rules and limits (ATSDR
2015 message mapping checklist; EPA/600/R-07/027: 3 key messages, 3 supporting facts, 27 words,
9 seconds, grade level minus 4, 1N = 3P, no absolutes), initial-statement elements, media holding
lines, first-48-hours steps, spokesperson tips and bridges, 24 of EPA's 77 reporters' questions,
audience concerns, and seven pre-scripted sets with FAQs and phase notes, each fact cited with a
confidence level. The CERC principles and press checklist are shared with `draft_press_materials`;
the coastal storm, heat, and shelter sets add the matching public messages from
`content/epr/hazard_health_actions.yaml`.

Checks: three key messages; three supporting facts each; key messages ≤ 27 words and ≤ 9
seconds at 3 words per second; empathy first; ≤ 12 words per sentence on average; reading grade
(the COMM-01 engine); no absolutes; action steps stated positively; three or four actions;
known/unknown/doing and next update in the holding statement; loaded words (shared with
COMM-04); client identifiers; safe messaging when suicide is mentioned. Agency translations are
read from `/data/templates/<language>/cerc_<id>.md` (ACS names are matched to folder names, e.g.
"Haitian" → `haitian_creole` or `haitian-creole`); other languages get a COMM-02 translator
brief. Nothing is machine-translated. An agency translation is used only when the English message
is the library's own: when you supply your own message map, action steps, or empathy statement,
the approved translation no longer matches, so a translator brief is prepared instead and a
caveat says so. A hazard name or alias matches as whole words ("measles at the library"), never
inside another word; a `hazard_description` selects a pre-scripted set only when it is exactly a
set's name, and otherwise a caveat suggests the set that may fit. Supporting facts left as
`[BRACKETED]` placeholders are listed as missing, not counted as written. Contact details found
in the request are reported in the privacy block.

### MCP prompt: `plan_tabletop_exercise`

Arguments (all optional): `scenario`, `area`, `exercise_date`, `roster_path`. Returns one message
that walks the model through an HSEEP tabletop exercise cycle for a Massachusetts local health
department: choose the hazard and describe the people most affected
(`hazard_vulnerability_assessment`, `get_active_hazard_alerts`, `build_community_health_profile`,
`language_access_profile`, and `get_electricity_dependent_population`, `heat_health_action_planner`,
or `search_ma_disease_control_guide` by scenario), design the exercise with objectives mapped to
PHEP and FEMA core capabilities (`design_hseep_exercise`), prepare player materials
(`generate_ics_forms`, `crisis_risk_communication_builder`, `continuity_of_operations_planner`,
`model_dispensing_site_throughput`, `plan_vaccination_clinic`, `functional_needs_shelter_intake`),
run the hotwash, and write the after-action report and improvement plan
(`build_after_action_report`). Staff contacts stay in the roster file; client details never go
into the chat. The message names the toolsets the workflow uses (`epr`, with `equity`, `env`,
`cd`, and `vax` for context) and tells the model to report, not retry, a tool whose toolset is
off.

## Phase 14: Environmental health and climate

<!-- Phase 14 tools go below this line. -->

<!-- Phase 14 Water (ENV-03, CHR-07). -->

### `drinking_water_quality_lookup` (ENV-03, toolset `env`)

| Argument | Default | |
|---|---|---|
| `town` | saved town | A Massachusetts city or town (counties and the state are refused: systems are listed town by town). |
| `system` | | A PWSID (`MA2002000`) or part of a system name; shows that system only. A name must match one system (ties are refused with the candidates). |
| `years` | 5 | Years of violation history (1–10). |
| `include_non_community` | false | Also list non-transient (schools, child care, workplaces) and transient (restaurants, campgrounds) systems; they are counted either way. |
| `private_wells` | automatic | Private well testing guidance; included automatically when the town's community systems report serving fewer than 90% of its ACS population (tool default) or there is none. |

**Output.** For each system: population served and connections, source (ground, surface,
purchased), wholesaler and school/child care flags, health-based violations whose compliance
period began in the window (contaminant, violation type, category, rule, status, return to
compliance, measured level, public notice tier; named from SDWIS reference codes, with the rule
and category when a code has no description), counts of other violations by category, the
number listed as Open, the latest lead and copper 90th percentiles against the 15 µg/L and
1.3 mg/L action levels, and a PFAS6 summary for treated water (latest result, highest result
and count above 20 ng/L in the last 12 months of data, and the highest sampling-location average
in the latest quarter, with not-detected counted as 0, a tool default). Flags list what needs
attention. The town summary compares the population served with the ACS population. Standards
(Massachusetts PFAS6 MCL and how MassDEP determines compliance; EPA's 2024 PFAS rule and the May
2026 proposals to rescind four MCLs and extend PFOA/PFOS compliance to 2031; the Lead and Copper
Rule and the 2024 Improvements), what residents can do (Consumer Confidence Reports; MassDEP's
PFAS advice for pregnant and nursing people and infants; EPA's lead steps), and private well
guidance (MassDEP Private Well Guidelines Tables 11 and 12, PFAS testing FAQ, certified labs,
board of health role, Table 9 arsenic/uranium towns, bedrock lead towns).

**Sources.** Live: EPA SDWIS/Fed through the Envirofacts data service
(`data.epa.gov/dmapservice/sdwis.water_system|violation|lcr_sample_result|lcr_sample`, filtered by
the four PWSID prefixes of the town: `MA` + MassDEP region digit + MassGIS town number) and the
EEA Data Portal API (finished-water PFAS6, 24 months), cached for a day, requested concurrently.
Fallback: snapshots `epa-sdwis-ma` (active systems, violations since 2016, lead and copper since
2016, reference codes) and `massdep-pfas6-ma` (24 months), retrieved 2026-11-11, built by
`xtask/scripts/build_water_snapshots.py`. Tribal systems (EPA Region 1 primacy; the Aquinnah
Wampanoag system) always come from the snapshot. Content: `content/env/drinking_water.yaml`.

**Privacy.** Public data only; SDWIS operator names, emails, and phones are never parsed or
returned. No local files.

**Limits.** SDWIS is quarterly and can lag MassDEP; a system is listed under the town in its
PWSID and may serve neighbors (and a neighbor's system may serve part of the town); population
served is self-reported. PFAS6 averages are informational, not compliance determinations.
Lead and copper action level exceedances are not violations. Revised Total Coliform Rule
violation codes have no SDWIS description and are named by rule and category.

### `community_water_fluoridation_status` (CHR-07, toolset `chronic`)

| Argument | Default | |
|---|---|---|
| `town` | saved town, else Massachusetts | A city or town, or "Massachusetts" for the statewide summary only (counties are refused). |
| `brief` | false | Write a DOCX board of health brief to `/data/output`. |

**Output.** The town's status on the DPH 2023 list (fully, partially, or not fluoridated; MWRA
communities; year begun; DPH's notes) and from CDC My Water's Fluoride's 2025 Fluoridation
Status Report (each community system assigned to the town: adjusted, consecutive, natural, or
non-adjusted; population; reported 2025 average fluoride for adjusted systems), whether the two
agree, and notes (duplicate CDC records that disagree, an adjusted system averaging 0 mg/L,
DPH notes such as Fall River's interim suspension). Statewide: CDC's November 2025 summary
(people and systems by status; 58.2% of people on community systems) and towns by DPH status.
Then the standards (PHS 0.7 mg/L; EPA MCL 4.0 and secondary 2.0 mg/L), each body's current
position side by side with its date (DPH, CDC, CPSTF as described by CDC, EPA, FDA, AAP), evidence
reviews (NTP 2024, JAMA Pediatrics 2025, Cochrane 2024, CDC 2024), where they differ (supplement
ages: DPH vs FDA vs AAP; safety at 0.7 mg/L; size of benefit), M.G.L. c. 111, § 8C in plain
language, draft board of health discussion points, and the bodies not included because their
sites could not be reached (ADA, ASTDD, AAPD, WHO, CPSTF's own page, HHS).

**Sources.** Snapshots `cdc-mwf-ma` and `dph-fluoridation-towns` (retrieved 2026-11-11; CDC
records matched to EPA SDWIS systems and towns by the builder, with outdated or duplicate CDC
PWSIDs labeled). Content: `content/chronic/fluoridation.yaml` (every position and finding cites
its own page or the PubMed record, with dates).

**Privacy.** Public data and reference content only; the brief contains no personal data.

**Limits.** DPH's list is from 2023 and CDC's report is state-reported and may show targets, not
test results; the water system is the best source. Positions are never ranked or chosen; this is
decision support for a board of health under c. 111, § 8C.

<!-- End of Phase 14 water. -->

<!-- Phase 14 EJ and climate (ENV-04, ENV-05). -->

### `environmental_justice_screen` (ENV-04, toolset `env`)

| Argument | Default | |
|---|---|---|
| `area` | saved town, else Massachusetts | City or town, county, or the state. |
| `ejscreen_path` | | An archived EPA EJScreen 2.3 (2024) block group CSV in `/data` (Massachusetts rows, or the national file under 150 MB). |
| `map` | false | Map of the EJ block groups (SVG and PNG) in `/data/output/maps`. |
| `table` | false | The area's EJ block groups (XLSX) in `/data/output`. |

**Output:** the area's 2020 Environmental Justice block groups (EEA layer, 2020 Census and ACS
2016–2020, updated November 2022, corrected June 2024) by criterion (minority, income, English
isolation) and by combination, with their 2020 Census population; for a town, EEA's official
municipal statistics (EJ and total block groups, people in EJ block groups, percent of residents),
with a caveat when the newer layer differs (Chelmsford); for a county or the state, the towns
ranked by people in EJ block groups (25 listed statewide). English isolation: the block groups
meeting that criterion, their limited-English households, the area's ACS limited-English
household percent, and the three largest language groups among residents with limited English.
SVI context: high-SVI tracts (top quarter in Massachusetts) and the EJ block groups in them. The
statutory criteria (St. 2021, c. 8, § 56), what a designation changes (MEPA designated geographic
area of 1 or 5 miles, EIR, advance notice, translation at 5% and interpreters above 10% of
limited-English residents, consulting Boards of Health on languages; EEA agency public
participation), DPH's four vulnerable health EJ criteria (heart attack, childhood blood lead,
low birth weight, childhood asthma at 110% or more of the state rate), and how to use them.

**Counting:** a block group split at a town line counts fully in each town it touches, as EEA
counts it; county and state totals count each block group once (2,599 block groups and 3,482,860
people statewide; EEA's published 2,604 and 3,487,681 count the split ones per town). State share
uses the 2020 Census population (7,029,917) and EEA's 4,985 block groups.

**EJScreen file:** read with the local-file guardrails. The `ID` column must hold 12-digit block
group codes (a column the identifier scan flags is accepted only when its values are
Massachusetts block group codes in known tracts; anything else is refused). For each `P_`
percentile column (13 environmental indicators, 13 EJ Indexes, demographic index, low income,
limited English) it returns the population-weighted average percentile (`ACSTOTPOP`) for all,
EJ, and other block groups in the area and the number at or above the 80th and 90th percentile,
plus the block groups with any EJ Index at or above the 80th percentile (EPA's former starting
point for review, not a designation). Block groups are placed in towns by their tract's main
town. Rows and individual values are never returned. State or national percentiles are read from
the file name (`StatePct`).

**Sources:** EEA/MassGIS 2020 EJ layer and municipal statistics; MassGIS 2020 Census towns; EEA EJ
Policy (2021); MEPA EJ protocols guide; DPH EJ Tool; EPA EJScreen 2.3 technical documentation and
column list; CDC/ATSDR SVI 2022; ACS 2020–2024. **Privacy:** public aggregate data; the optional
EJScreen file gives averages and counts only; files go to `/data/output`. **Limits:** EPA's EJScreen
archive and the CDC/ATSDR Environmental Justice Index download (eji.cdc.gov) could not be reached
when this was built, so neither is bundled; the map polygons are simplified (15 m).

### `climate_health_vulnerability_profile` (ENV-05, toolset `env`)

| Argument | Default | |
|---|---|---|
| `area` | saved town | One city or town (counties and the state are refused). |
| `document` | false | Word (DOCX) profile in `/data/output`. |

**Output** (each indicator with value, Massachusetts value, ratio, source, and vintage):

- Heat: extreme heat days a year (2018–2022), tree canopy, and impervious surface (NLCD 2021)
  from the CDC/ATSDR Heat and Health Index 2024. HHI publishes ZIP Code Tabulation Area values;
  town values are averages weighted by each ZCTA's land area in the town, and the state value
  weights every ZCTA by land area. The ZCTAs with their HHI national ranks are listed.
- Flooding: NFIP claims since 1978 (count, paid, dollars, peak year), claims in the last ten
  complete years, claims per 1,000 2020 Census housing units vs the state rate; NFIP
  participation, current flood map date, Community Rating System class and discount (Community
  Status Book).
- Social vulnerability: percent of residents in EJ block groups (EEA) and high-SVI tracts; ACS
  older adults, young children, disability, poverty, no vehicle, limited English, no internet,
  renters; PLACES asthma, COPD, coronary heart disease, diabetes, obesity, disability types,
  loneliness, utility shutoff threat; HHS emPOWER electricity-dependent beneficiaries.
- Planning: MVP region, MVP planning reports posted on mass.gov, designated through prior work,
  FY24 MVP 2.0 pilot; FEMA hazard mitigation plans for the town (status, approval, expiration).
- Most important vulnerabilities (at most 8), ranked by distance from the state value, by
  **tool-default rules** stated in the output: percent indicators at least 1.2 times the state
  value (unreliable ACS estimates skipped), heat days 1.1 times, tree canopy 0.75 times or less,
  impervious 1.5 times, NFIP claims rate 2 times or 25+ recent claims, any top-quarter SVI tract,
  50% or more of residents in EJ block groups, emPOWER rate 1.2 times. A planning flag is raised
  when no approved plan is listed or the plan has expired or expires within 12 months
  (44 CFR 201.6(d)(3): revise and resubmit within 5 years).
- Health effects of extreme heat and flooding, and who is at risk (shared hazard content).

**Live data:** the NFIP Community Status Book and FEMA hazard mitigation plan statuses (OpenFEMA,
cached a day, bundled fallback); emPOWER as in `get_electricity_dependent_population`. Everything
else is bundled, so the profile works offline. **Sources:** CDC/ATSDR HHI 2024 (data and
technical documentation), OpenFEMA NfipClaims v3, NfipCommunityStatusBook, and
HazardMitigationPlanStatuses, mass.gov MVP pages, eCFR 44 CFR 201.6, EEA EJ layer, SVI 2022, ACS,
PLACES 2025, emPOWER. **Privacy:** public aggregate data only; the DOCX goes to `/data/output`.
**Limits:** NFIP claims count insured losses only and are not a flood-risk map; FEMA NFHL flood
zone areas by town and a verified town temperature series were not available (ResilientMass and
CDC's Tracking Network were blocked); mass.gov does not list designated MVP communities.

<!-- End of Phase 14 ejclimate. -->

<!-- Phase 14 Vectors and housing (ENV-06, ENV-07). -->

### `vector_borne_disease_risk` (ENV-06, toolset `env`)

| Argument | Default | |
|---|---|---|
| `area` | saved town, else Massachusetts | Town, county, or the state. |
| `season` | current season (June–October), else the latest | 2022 to the latest DPH workbook. |
| `audiences` | all | `public`, `schools`, `camps`, `outdoor_workers`, or `all`. |
| `notice` | false | One-page DOCX notice in `/data/output` (first audience given). |

**Output.** EEE and West Nile virus risk levels for the town (or the number of towns at each
level and the towns at EEE moderate or higher or WNV high or higher, for a county or the state),
with the date DPH last updated them and whether they are current; for the highest level in the
area, what it means, what residents can do (DPH's dashboard text), and what DPH's 2026 Arbovirus
Surveillance and Response Plan recommends (Tables 1 and 2, shortened); positive mosquito
samples and animal cases (town or county) and human cases (county of residence only, counts);
the area's final levels in earlier seasons; DPH's recommended cancellation window for evening
activities when EEE is high or critical during the season (2026 schedule, Appendix 2); Lyme
disease (probable), babesiosis, and anaplasmosis (confirmed and probable) cases and DPH
incidence rates per 100,000 by county, 2022–2025, with the Massachusetts rate, the change from
the previous year, and the ratio to the state, and DPH's ten-year statewide totals; emergency
department visits with a tick-borne disease diagnosis by county (DPH syndromic surveillance,
2021 to the current year); current-year babesiosis and anaplasmosis counts for Massachusetts from
NNDSS (via `get_notifiable_disease_trends`); and cited prevention messages for each audience.

**Sources.** Live: DPH's Massachusetts arbovirus update page and the season's "EEE and WNV risk
level and data" workbook it links (cached 6 hours in season, 7 days outside it), parsed with the
server's spreadsheet reader. Snapshots: `dph-arbovirus-risk` (2022–2026 workbooks) and
`dph-tickborne-surveillance` (DPH Tick-borne Disease Surveillance Summaries 2022–2025 and the
syndromic reports), built by `xtask/scripts/build_vectorhousing_snapshots.py`. Content:
`content/env/vector_borne.yaml` (DPH, CDC, NIOSH, 105 CMR 430.000).

**Season handling.** From June through October the tool tries the live workbook; levels are
called current only when they come live or from the cache for this year's season. Outside the
season (and when the live page cannot be reached) it shows DPH's last levels with their date and
says plainly that the season is over (or has not started) and that the levels are not current
risk. During the season, a bundled copy is labeled as possibly out of date.

**Privacy.** Public data only. Human cases are counted by county as DPH publishes them; their
age, sex, and clinical presentation are not kept.

**Limits.** DPH sets risk for focal areas, so a town can be at moderate or high risk without its
own positive finding. DPH publishes tick-borne disease cases by county only (town figures are
maps); Dukes and Nantucket are combined. Lyme counts from 2022 on follow the new case definition
and cannot be compared with earlier years. Rates are DPH's as published; counts under 20 are
flagged as unstable. NNDSS had no 2025–2026 Massachusetts babesiosis or anaplasmosis rows when
checked, so the current-year line may say so. The cancellation schedule covers the 2026 season;
DPH issues a new one each year.

### `housing_conditions_health_guide` (ENV-07, toolset `env`)

| Argument | Default | |
|---|---|---|
| `conditions` | all | Names or the tenant's words: mold, leak, pests, roaches, bed bugs, no heat, hot water, lead paint, smoke or carbon monoxide alarms, trash. Matched on whole words. |
| `area` | saved town | Town or county for local housing facts (ACS). |
| `teaching_sheets` | true | Plain-language tenant sheets in `/data/output`. |
| `format` | docx | `docx` or `pdf`. |
| `languages` | | Agency-approved translations from `/data/templates/<language>/housing_<sheet>.md`; no machine translation. |
| `households_path` | | CSV/XLSX in `/data`, one row per household (name, address, unit, town, phone, owner, problems, date noticed, language). |
| `letters` | both | `inspection_request`, `repair_request`, or `both` (needs `households_path`). |

**Output.** For each condition: the 105 CMR 410.000 sections and what they require (moisture and
mold 410.500, 410.420, 410.220, 410.620(E); pests 410.550, 410.540; heat 410.160, 410.180; hot
water and utilities 410.150, 410.130, 410.003(C); lead 410.470 and M.G.L. c. 111, § 197; smoke and
carbon monoxide alarms 410.330, 410.170; trash 410.560, 410.570), owner and occupant
responsibilities, whether 410.630(A) always deems it a condition that may endanger health, health
effects and who is most at risk (CDC, EPA), and practical advice. Then the inspection process
(410.600–410.680: how to ask, 1 or 5 business days, comprehensive inspection, orders within 12
hours or 7 days, correction within 24 hours or 30 days, reinspection, hearings 410.800–410.860),
the 18 conditions always deemed to endanger health, the saved board of health phone, tenant
protections (M.G.L. c. 186, § 18; c. 239, § 8A; c. 111, § 127L; DPH's Notice of Occupants' Legal
Rights and Responsibilities) with a not-legal-advice notice, and local housing facts (built
before 1980 and 1950, renters, cost burden, crowding). Teaching sheets (one on getting an
inspection, one per condition) are scored with the COMM-01 readability engine (median grade;
target 8). The two letter templates are returned for review.

**Privacy.** The letters follow ADR 0008: the AI sees only the templates; household details are
read from the local file and written only to `/data/output`; the response has counts (households,
letters, blank fields, languages) and file names. The board of health phone and town come from
saved settings.

**Limits.** Content is drafted from the regulation published in Mass. Register #1498 (6/23/2023)
and pending review. The tenant protections summarize the statutes and DPH's notice and leave out
conditions; refer tenants to legal aid. Local and city housing codes may add requirements.

<!-- End of Phase 14 vectorhousing. -->

<!-- Phase 14 Chemicals and food (ENV-08, CHR-06). -->

### `chemical_exposure_reference` (ENV-08, toolset `env`)

| Argument | Default | |
|---|---|---|
| `chemical` | (required) | A name, synonym, trade name, or abbreviation ("benzene", "TCE", "perc", "PFOA", "PFAS"), or a CAS Registry Number ("71-43-2"; the check digit is verified). |
| `scenario` | none | `spill_or_release`, `indoor_air`, `contaminated_site`, `drinking_water`, or `workplace`: adds scenario notes, links, and whom to call. |

Looks the chemical up in four bundled snapshots: the NIOSH Pocket Guide (677 entries: synonyms,
CAS, DOT ID and ERG guide, conversion, NIOSH REL, OSHA PEL, IDLH, physical description, exposure
routes, symptoms, target organs, cancer site, first aid, page link), the NIOSH IDLH table (current
values, which can differ from the Pocket Guide's), the ATSDR MRL table (October 2026: route,
duration, MRL, total factors, endpoint, draft or final), and the ATSDR ToxFAQs index (links to the
web page and two-page PDF; the text is not copied). Records are joined by CAS number, by ATSDR's
substance id, or by identical names (NIOSH lists chlordane under 57-74-9, ATSDR under 12789-03-6).
Matching: CAS number; exact name, synonym, parenthetical abbreviation, or singular of a plural
name; content aliases (PFAS); then fuzzy spelling (accepted at 0.86 or more, a tool default, and
flagged "check this is the chemical you meant"). When different chemicals score within 0.02 of
each other the call fails with the candidates ("arsenic": inorganic or organic compounds); below
the threshold it fails with suggestions and a pointer to the Safety Data Sheet.

Every answer starts with the framing (also the first caveat): occupational limits (REL, PEL, IDLH)
are for adult workers over a work shift and are not safe levels for community or residential
exposure; MRLs are screening levels, not cleanup levels or diagnostic thresholds. "How to read
these numbers" cites the NIOSH Pocket Guide introduction, the NIOSH IDLH page, EPA's AEGL page,
ATSDR's MRL pages, and ATSDR's Public Health Assessment Guidance Manual. Whom to call (from
`content/env/chemical_exposure.yaml`, every number read from the cited official page): 911 (local
fire department), Poison Control 1-800-222-1222, MassDEP's 24-hour spill line 1-888-304-1133, the
National Response Center 1-800-424-8802, the National Pesticide Information Center, DPH's
toxicology division and Indoor Air Quality unit (617) 624-5757, MassDEP's main office, and the
local board of health. Without a scenario it lists 911, Poison Control, MassDEP, DPH toxicology,
and the board of health. Caveats flag provisional (draft) MRLs, a missing MRL ("does not mean the
chemical is safe"), an IDLH that differs between the Pocket Guide and the current table, fuzzy
matches, and review status. Decision support; no local data are read.

Limits: reference tables only (no AEGLs, drinking water standards, or soil screening values; use
`drinking_water_quality_lookup` for water). OSHA's pages could not be reached, so OSHA is named
through the Department of Labor Standards page. Snapshots: `niosh_npg.csv`, `niosh_idlh.csv`,
`atsdr_mrls.csv`, `atsdr_toxfaqs.csv` (`xtask/scripts/build_chemfood_snapshots.py`).

### `food_access_mapper` (CHR-06, toolset `chronic`)

| Argument | Default | |
|---|---|---|
| `area` | saved town, else Massachusetts | Town, county, or the state. |
| `distance` | `straight_line` | `straight_line` or `driving` distance to the nearest SNAP-authorized store. |
| `definition` | any | `one_and_ten`, `half_and_ten`, `one_and_twenty`, or `vehicle`: the definition for the tract list and the map. |
| `pantries_path` | | CSV/XLSX in `/data` of food pantries and meal sites (organizations): name, type, address, town, ZIP, hours, phone, website, languages, latitude, longitude. |
| `snap_retailers_path` | | CSV in `/data` from USDA's SNAP Retailer Locator (store name, store type, city, state, ZIP, latitude, longitude, end date). |
| `hip_vendors_path` | | CSV/XLSX in `/data` of HIP farm vendors (name, type, address, town, hours), for example built from DTAFinder.com. |
| `map` | true for a town or county | Tract map (SVG and PNG) in `/data/output/maps`. |
| `report` | true | DOCX resource list and XLSX workbook in `/data/output`. |

Data: the bundled `usda-food-access-ma` snapshot of the USDA ERS Food Access Research Atlas. The
2025 SNAP-authorized Retailer Access Map (SRAM; STARS stores as of June 2025; released July 2026)
is on 2020 census tracts, the project's tract geography, and gives the four low-income/low-access
(LILA) flags by straight-line and driving distance, low-access people and low-income people,
households without a vehicle, SNAP households, and children and older adults beyond 1 mile. The
2019 Large Retailer Access Map (LRAM, formerly the Atlas; supermarkets, supercenters, and large
grocery stores) is on 2010 tracts; the builder carried its LILA flags to 2020 tracts with the
Census 2020-to-2010 tract relationship file (share of each 2020 tract's land in a flagged 2010
tract), and a 2020 tract counts when the share is at least 50% (tool default). A town uses the
tracts lying mainly in it (as the SVI tool does). Output: for each definition the LILA tracts and
people in them (SRAM) with the LRAM comparison; context (population, households, low-income
tracts, households without a vehicle or receiving SNAP, ACS poverty and no-vehicle percentages,
LILA tracts in the top quarter of the Massachusetts SVI); the LILA tracts (up to 60; all in the
workbook) with town, poverty, definitions met, and SVI; HIP and SNAP facts from DTA (HIP $40/$60/$80
a month by household size, vendor types, DTAFinder, EBT Customer Service; DTA Assistance Line,
timelines, public charge); and how to get each local file not given.

Privacy: local files go through `phn_tools::localdata`, and a file with person columns (first or
last name, client, patient, participant, recipient, member, date of birth, SSN, MRN, health plan
or account numbers) is refused without echoing values. Only counts are returned: sites by town,
by a fixed set of kinds (the file's own labels are not repeated), sites inside LILA tracts and
LILA tracts without a site (when coordinates are given). Site names, addresses, and phones go
only to the DOCX and XLSX in `/data/output`. SNAP retailer rows outside Massachusetts or with an
end date are left out.

Limits: no live fetch (ERS's map services at gisportal.ers.usda.gov are blocked from the build
environment; the Atlas changes every few years). The SNAP Retailer Locator (www.fns.usda.gov,
now www.fna.usda.gov) and DTAFinder.com could not be reached, so those lists are files the nurse
adds. Distance is not the only barrier to food (cost, hours, transportation, culturally
appropriate food).

<!-- End of Phase 14 chemfood. -->

<!-- End of Phase 14 tools. -->

## Phase 15: Chronic disease prevention and community assessment

<!-- Phase 15 tools go below this line. -->

<!-- Phase 15 screen (CHR-02, CHR-03). -->

<!-- End of Phase 15 screen. -->

<!-- Phase 15 prevent (CHR-04, CHR-05). -->

### `preventive_services_lookup` (CHR-04, toolset `chronic`)

| Argument | Default | |
|---|---|---|
| `age_years` | required | Whole years, 0 to 120. No names or other identifiers. |
| `sex_at_birth` | required | `female` or `male` (the recommendations are keyed to sex at birth or organs present). |
| `pregnancy_status` | not given | `not_pregnant`, `pregnant`, or `postpartum`. When left out the person is treated as not pregnant and the pregnancy-specific recommendations are not evaluated (the response counts and names them). |
| `risk_factors` | none | `tobacco_use` (current, former, never), `pack_years`, `years_since_quit`, `bmi`, `bmi_percentile`, `cardiovascular_risk_factors` (hypertension, elevated blood pressure, dyslipidemia, diabetes, metabolic syndrome; an empty list means none), `ten_year_cvd_risk_percent`, `sexually_active`, `postmenopausal`, `fair_skin`, `home_water_low_in_fluoride`, and `increased_risk_for` / `not_at_increased_risk_for` lists (HIV, hepatitis B, syphilis, STIs, chlamydia and gonorrhea, TB, falls, breast cancer, BRCA-related cancer, preeclampsia, perinatal depression, osteoporotic fracture). |
| `include_myhealthfinder` | true | Ask ODPHP's MyHealthfinder (live, no key) for its list for the same age, sex, and pregnancy; skipped offline. |
| `write_handout` | false | Plain-language client handout (DOCX or PDF, reading grade checked) with no name or identifier, to `/data/output`. |
| `format` | `docx` | `docx` or `pdf`. |

Data: the bundled `uspstf-ab-recommendations` snapshot, built by
`xtask/scripts/build_prevent_snapshots.py` from the Task Force's published recommendation
statements (the latest statement per topic, 2016 on; 53 grade A or B recommendations from 46
statements), read as PubMed abstracts through the E-utilities because the USPSTF web site and API
(`www.` and `data.uspreventiveservicestaskforce.org`) were blocked from the build network. Grade,
population wording, and recommendation sentence are copied from the abstract, and the builder
refuses to write a grade that the text does not show. Dates are journal publication dates. The
project's layer (`content/chronic/preventive_services.yaml`) adds the age, sex, pregnancy, and
risk-factor criteria the tool can apply, the plain-language client summaries, and the
Massachusetts program options.

How it decides: each recommendation has one or more alternatives (age range, sex at birth,
pregnancy status, and required risk-factor flags). It goes under `applies` when an alternative is
fully met, under `needs_review` when it fits on age, sex, and pregnancy but a required risk
factor was not given (the response says which input is missing), and is counted in
`not_matched_count` when age, sex, pregnancy, or a risk factor you gave rules it out. Leaving a
risk factor out never removes a recommendation. Everything else in the published population
("average risk", "no known disease", "asymptomatic") is shown as the Task Force's own wording plus
a `confirm` note for the nurse to apply. Where a statement gives no age range (for example "women
of reproductive age" or "adolescents") the tool chose one and says so in `assumption`.

Output: for each recommendation the USPSTF grade, the statement date and DOI link, the Task
Force's population and recommendation text, the `confirm` note, a client summary, and related
Massachusetts programs; the Massachusetts options (the Massachusetts Breast and Cervical Cancer
Program with its eligibility as DPH lists it, sites, and toll-free number; the MA Colorectal
Cancer Control Program; 1-800-QUIT-NOW); MyHealthfinder's topics for the same person when online;
and what is not covered. It is the grade A and B list only (not ACIP vaccines, HRSA lists, or
grade C).

Privacy: the request has only age, sex at birth, pregnancy status, and risk factors; no local
files are read. When MyHealthfinder is used, age, sex, and pregnancy status (no identifier) go to
odphp.health.gov.

Limits: no USPSTF API adapter (the API was unreachable and its response format could not be
checked; no `PHN_USPSTF_API_KEY` exists), so a statement updated after the snapshot date is not
seen: the answer says to check the Task Force's page. Statements before 2015 (2008 newborn
screening statements) are not included. MyHealthfinder has no offline copy. The content is
`status: pending` until the nurse reviews it.

### `asthma_home_assessment` (CHR-05, toolset `chronic`)

| Argument | Default | |
|---|---|---|
| `sections` | all | Any of `checklist`, `control`, `action_plan`, `services`. |
| `town` | saved town | Massachusetts city or town, to mark home visiting programs that name or are located in it. |
| `control_answers` | | Counts to classify (needs `control`): `symptom_days_past_30`, `symptoms_throughout_the_day`, `nights_waking_past_30`, `rescue_uses_per_day` or `no_inhaler_medicine_past_3_months`, `activity_limitation_past_12_months`, `age_years`. |
| `write_documents` | true | Blank home visit checklist and blank asthma action plan to `/data/output`. |
| `format` | `docx` | `docx` or `pdf`. |
| `clients_path` | | CSV/XLSX in `/data`, one row per person, with the prescriber's plan details: name, doctor, doctor phone, hospital phone, plan date, best peak flow, daily medicine, dose and timing, before-exercise medicine, quick-relief medicine and puffs, repeat minutes, maximum doses, oral steroid, mg and days, call hours, red-zone puffs, ambulance phone, triggers (needs `action_plan`). |

Data: `content/chronic/asthma_home.yaml` (status pending), built from the CDC, EPA, and HUD
"Home Characteristics and Asthma Triggers: Checklist for Home Visitors" (June 2018) and EPA's
"Asthma Triggers: Gain Control"; NHLBI's Asthma Action Plan (NIH Publication No. 20-HL-5251,
February 2021); the control table DPH prints in "Factors Associated with Asthma Management in
Massachusetts" (2022), which adapts NHLBI's EPR-3 for the CDC Asthma Call-back Survey; and DPH's
Massachusetts Asthma Home Visiting Program Directory (last updated by DPH in March 2025). Housing
code facts are not repeated: each checklist section names a condition in the Phase 14 housing
content (`env-housing-conditions`: moisture and mold, pests, heat, smoke and CO alarms) and the
tool reads its 105 CMR 410 sections from there at run time.

Control: the table has four items (symptom days in the past 30 days, nights waking, rescue
inhaler uses per day, limits on usual activities in the past 12 months), each with well controlled,
not well controlled, and very poorly controlled criteria as DPH prints them; the classification is
the most severe item answered. Where the table's ranges meet (0.29 and 1.00 uses a day) the more
severe category is used. It is a screening classification: no lung function or exacerbation
criteria, DPH applied the age 12 and older cut points to children, and EPR-3 itself (NCBI
Bookshelf, nhlbi.nih.gov) and GINA could not be reached, so the clinical grids are not bundled. The
Asthma Control Test is licensed (QualityMetric) and is not reproduced; the response says so and
gives the ACT and ACQ cut scores NHLBI's 2020 update prints.

Action plan: the NHLBI zones (green, yellow, red, danger signs) as a template with
`{{placeholders}}`; peak flow zones are worked out from a best peak flow with NHLBI's percentages
(80 percent or more; 50 to 79; under 50). Template + local fill (ADR 0008): the filled plans for
the people in `clients_path` go to `/data/output` only, medicines and doses appear only if they are
in the file, and the response has counts and the file name.

Services: DPH's seven-program directory (organization, location, regions served, and pediatric,
adult, and older adult programs), with the town-fit label, DPH's description of the CHW-led
model, and other DPH-listed resources (APCP email, MAAP, Massachusetts Breathing Better, the IPM
toolkit, the Health Promotion Clearinghouse action plans). DPH lists regions, not towns, and no
phone numbers.

Privacy: local files go through `phn_tools::localdata`; person details in the file are written
into the action plan file only. Control answers are counts typed in the request.

Limits: MassHealth coverage of asthma home visits, local board of health or housing programs, and
program phone numbers were not checked; the Massachusetts Health Promotion Clearinghouse (the
DPH action plan forms) and nhlbi.nih.gov were blocked; the checklist is shortened from the
original.

<!-- End of Phase 15 prevent. -->

<!-- Phase 15 cha (EQ-08, EQ-09, EQ-10). -->

<!-- End of Phase 15 cha. -->

<!-- Phase 15 geo (EPI-13). -->

### `geocode_and_aggregate_addresses` (EPI-13, toolset `epi`)

| Argument | Default | |
|---|---|---|
| `path` | (required) | CSV or spreadsheet in `/data`, one address per row. |
| `sheet` | first | Sheet of a workbook. |
| `address_column` | found from the headers (`Street Address`, `Address`, `Home Address`, `Street`, ...) | The street address, or the whole address in one cell (`12 Elm St, Amherst, MA 01002`). |
| `town_column` | found from the headers (`Town`, `City`, `Municipality`, ...) | City or town. |
| `zip_column` | found from the headers (`ZIP`, `ZIP Code`, `Postal Code`, ...) | ZIP code (a spreadsheet's four-digit `1002` is read as `01002`). |
| `town` | none | One Massachusetts town for every row without a town value. |
| `level` | finest possible | `town`, `tract`, or `block_group`. Falls back to town, and says why, when no address could be placed on a street. |
| `zip_min_share` | 0.9 | A row that has only a ZIP code is counted in a town when that town holds at least this share of the ZIP code's addresses (0.5 to 1). |
| `write_matched_file` | false | Also write `/data/output/<file>_geocoded.csv` (row number, the address as written, match level, town, tract, block group, latitude, longitude). Identifiable; only its name is returned. |

Locates the addresses **offline**, inside the container, against MassGIS Master Address Data
points and Census TIGER address ranges (ADR 0011), and returns only counts. Nothing is sent to a
geocoding service and nothing from the file (no address, coordinate, name, or row) is returned.
It reads three columns: address, town, and ZIP code. A column the PHI detector takes for a name,
date of birth, phone number, SSN, ID number, or free text is refused when it is named as one of
the three (the message names the column and the kind, never a value); the file's other columns
are not read, so a client list can be used as it is.

Address handling: case-insensitive; USPS street-type abbreviations (`St`, `Street`, `Str`, `Ave`,
`Rd`, ...), directions (`N`, `North`), ordinals (`First`, `1st`), `Saint`/`St`, `Mount`/`Mt`, route
numbers (`Route 9`, `State Rte 9`), unit designators (`Apt 3B`, `Unit 2`, `#4`, `2nd Floor`, `Rear`)
removed, house-number suffixes (`12A`, `12 1/2`) and ranges (`12-14`), a city, state, and ZIP
written after the street, PO boxes (no street position claimed), and village names (Florence,
Hyannis, Indian Orchard). A street typed without its type or with the wrong one is accepted when
only one street in the town has that name.

How precisely each row was located, best first: **exact address point** (a MassGIS point for the
house number); **interpolated** (between neighboring points in the same block group, or along a
TIGER address range); **street only** (the street, not the number; the block group is reported
only when the whole street is in one); **ZIP code only** (one town holds at least `zip_min_share`
of the ZIP code; approximate, because ZIP codes cross town lines); **town only** (from a town
column or the address); **not located** (with the reason). The response gives the count at each
level, the match rate to a street position, and the counts by town, 2020 census tract (with its
name and GEOID), or block group. Rows that could only be counted in a town appear as one line
at tract and block group level.

Privacy: counts below the suppression threshold (default 5, your setting) are hidden in every
table, with complementary suppression because a total is shown, and areas are listed by name so
the order reveals nothing. The match rate is omitted when one of its counts is small. Counts are
rows, not people or households; divide by a population from ACS for rates.

Data: the **full street index is optional** (35.5 MB, 351 towns, 2.2 million address points,
built in about two minutes by `xtask/scripts/build_geocoder_index.py --full OUT_DIR` and installed
in `/data/geocoder/` or `/cache/geocoder/`; see ADR 0011). To install it, on any computer with Python 3.10 and numpy run `python3 -I xtask/scripts/build_geocoder_index.py WORK_DIR --full OUT_DIR` (it downloads about 350 MB from MassGIS and the Census Bureau into WORK_DIR), then put the OUT_DIR folder where the container sees it: inside your mounted data folder as `geocoder/`, or mount it at `/cache/geocoder` (`-v /path/to/OUT_DIR:/cache/geocoder:ro`). Without it the bundled sample index
(Alford, Amherst, Northampton, and Pelham; real data) places addresses in those four towns and
every other row is counted by town (town column, a town written in the address, or the bundled
ZIP-code and village-name crosswalk). An installed index is checked against its manifest
checksums; a mismatch is reported and the sample used.

Limits: Boston has no MassGIS address-point download, so its addresses are placed from TIGER
ranges only (interpolated or street only). Interpolated positions assume numbers are evenly spaced
on a straight segment, and a point is usually a building or parcel center, so it can lie in a
different block group than the front door; where a town has two streets with the same name (villages) and the same house number, the index keeps one point (23 of 20,328 test addresses). No intersections, named places, or spelling correction
of street names; PO box ZIP codes are not in the ZIP crosswalk. At most 500,000 rows. Sources:
MassGIS Master Address Data (town exports of 2026-11-09), TIGER/Line 2025 (address ranges and
2020 block groups), `ma_geocoder_sample.idx`, `ma_geocoder_crosswalk.csv`
(`xtask/scripts/build_geocoder_index.py`); MassGIS's data disclaimer could not be retrieved, so its
terms are unverified (credit MassGIS).

<!-- End of Phase 15 geo. -->

<!-- End of Phase 15 tools. -->

## Phase 16: Program planning, evaluation, and evidence

<!-- Phase 16 tools go below this line. -->

<!-- Phase 16 design (PLAN-01, PLAN-02, PLAN-05). -->

<!-- End of Phase 16 design. -->

<!-- Phase 16 interventions (PLAN-03, PLAN-04, PLAN-11). -->

### `find_evidence_based_interventions` (PLAN-03, toolset `planning`)

| Argument | Default | |
|---|---|---|
| `topic` | required | Health topic or problem, for example "childhood obesity" or "colorectal cancer screening". Related words are searched too (smoking and tobacco, drinking and alcohol, and so on; 26 topic groups). |
| `population` | | For example "adolescents" or "older adults". Used to rank, not to exclude. |
| `setting` | | For example "school", "worksite", "health care", "community", "home". Used to rank. |
| `kinds` | all | `recommendation`, `systematic_review`, `economic_review`. |
| `evidence` | `any` | `any`, `strong_or_recommended` (abstract says strong or sufficient evidence, or that the Task Force recommends), or `insufficient`. |
| `max_results` | 8 | 1 to 20. |
| `include_live` | true | Also search PubMed for newer papers (only the topic words are sent); skipped offline, and any failure leaves the snapshot answer standing. |

Data: the bundled `cpstf-publications` snapshot, built by
`xtask/scripts/build_interventions_snapshots.py` (with `--check`) from PubMed: the 157 journal
papers indexed under the Corporate Author "Community Preventive Services Task Force" (or its
earlier name, "Task Force on Community Preventive Services"), of which 145 are bundled (eight
methods papers and four other items are left out). Each record keeps what PubMed shows (title,
journal, publication year and month, DOI, MeSH headings, the paper's kind from its title) and,
from the abstract, the sentences that use rating words ("strong evidence", "sufficient
evidence", "insufficient evidence", "recommends", "recommends against") and one short excerpt
each of the findings, the conclusion, and any implementation, barrier, or cost sentence. A rating
appears only when those words are in the abstract (or, for a few recommendation titles, in the
title); otherwise the result says the PubMed record gives none. The Rust rule that reads rating
words is tested against every sentence the builder stored. The Community Guide
(`www.thecommunityguide.org`) and What Works for Health (`www.countyhealthrankings.org`) both
answered the build network's connection request with HTTP 403, so neither site nor its terms of
use was read and What Works for Health has no data here; the result lists what to check on each
site. `content/planning/evidence_sources.yaml` holds the rating key, the topic synonyms, and
those notes.

Output: matches ranked by topic words in the title, MeSH headings, and abstract excerpts; for
each, the evidence wording, quotations, finding, conclusion, implementation notes, a reminder to
check for updates when the paper is eight or more years old, and links. Privacy: no local files;
the topic words go to PubMed only for the live search.

Limits: this is not the Community Guide's list of recommendations (its pages, and ratings that
have no journal paper, are missing), and a paper can be updated or archived after it was
published. Ratings are per paper, and a paper can cover several interventions.

### `apply_behavior_change_theory` (PLAN-04, toolset `planning`)

| Argument | Default | |
|---|---|---|
| `problem` | required | The health problem or behavior and what is going wrong. Describe a population, never a person (identifier-like text is refused). |
| `population`, `setting` | | Who the program serves and where it runs. |
| `barriers` | none | Barriers and facilitators in your own words, one per entry (up to 15). Each is matched to constructs. |
| `program_components` | none | Planned activities, one per entry (up to 15). Each is matched to strategies that share a word. |
| `theories` | auto | Ids, abbreviations, or names (`hbm`, `sct`, `tpb`, `ttm`, `sem`, `doi`, `comb`, `sdt`), or `all`. Auto picks up to three whose constructs match your words and adds the Socio-Ecological Model when none looks beyond the individual. |
| `write_document` | false | Word or PDF worksheet in `/data/output`. |
| `format` | `docx` | `docx` or `pdf`. |

Data: `content/planning/behavior_theories.yaml` (schema
`content/schemas/planning_behavior_theories.schema.json`): eight theories and frameworks, 48
constructs. Construct names and definitions are paraphrased from nine open-access papers (full
text read through PubMed Central) and CDC's social-ecological model page; the National Cancer
Institute's Theory at a Glance was blocked and the original theory papers were not read. Each
strategy is marked `source` (the Social Cognitive Theory strategies come from a published
construct-to-behavior-change-technique table, the COM-B strategies from the Behaviour Change
Wheel's published links, the Socio-Ecological Model examples from CDC) or `authored` (this
project's suggestion). Measure ideas are what to count or ask, not validated instruments; no
survey instrument is copied. Output: the theories shown with why, constructs (the ones whose
signal words match your description first, with the words that matched), strategies and
measures, a barrier-to-construct table, a component-to-strategy table, and notes on choosing
among theories. Matches are leads, not a diagnosis.

### `track_program_performance` (PLAN-11, toolset `planning`)

| Argument | Default | |
|---|---|---|
| `path`, `sheet` | required | CSV/XLSX in `/data`: one row per period (wide), one row per period and measure (long), or one row per record (`count_rows`). |
| `period_column` | required | A date, "2025-Q1", "Jan 2025", "FY26 Q2", or any label. Dates roll up to `date_period` (`month` default, `quarter`, `week`); text periods sort by year and quarter or month when every label has one, otherwise by first appearance. |
| `measure_column`, `value_column`, `denominator_column` | | Long layout: the measure name, the value (a count or a numerator), and the denominator for percentages. Without `value_column`, rows are counted per period and measure. |
| `count_rows` | false | Count the rows per period as the measure "Records" (give it a target). |
| `targets` | required | Up to 12: `measure`, `target`, `direction` (`higher_is_better` default or `lower_is_better`), `kind` (`count` default or `percent`), `denominator` (wide layout column), `basis` (`cumulative` default or `each_period`), `baseline`, `unit`. |
| `periods_in_program` | | Periods in the program or grant year, for pace (4 quarters, 12 months). |
| `on_track_fraction` | 0.9 | A cumulative count is "on track" at this share of the pace needed (this tool's convention; use your funder's rule). |
| `write_report`, `write_workbook`, `write_charts` | true | Report (`format`: `docx` or `pdf`), workbook, and one PNG chart per measure in `/data/output`. |

Method: a cumulative count adds the periods; a cumulative percentage pools numerators and
denominators (not the mean of period percentages). Progress for a count is total / target; for a
percentage with a baseline it is (current - baseline) / (target - baseline); without a baseline
it is current / target for higher-is-better and not computed for lower-is-better. Pace for a
cumulative higher-is-better count: expected = target x periods reported / `periods_in_program`;
Met at or above target, On track at or above `on_track_fraction` x expected, otherwise Behind.
Trend flags run on the periods with data: a run chart (the median and the Anhoej run and
crossing rules, `phn_epi::spc`) or, for percentages with at least eight periods, a p chart; fewer
than six periods get no flags, and fewer than ten are flagged as weak. The workbook has a Summary
sheet (targets, Current, Progress, Expected, Status, and Latest as live formulas) and a Data sheet
(numerators, denominators, and percent formulas down to row 101), so next quarter's row can be
typed in.

Privacy: the file is read through `phn_tools::localdata`, and only the named columns are read;
identifier columns are refused, other columns (including contact people) are never read or
returned, and period labels that look like identifiers are refused. Only counts by period are
returned: a count from 1 to the threshold minus 1 (or, for a percentage, a numerator, a
denominator, or the remainder that small) is hidden, and the measure's totals, progress, center
line, and status are then withheld because they would reveal it. The report, workbook, and charts
in `/data/output` hold the full values from the nurse's file.

<!-- End of Phase 16 interventions. -->

<!-- Phase 16 appraise (EBP-03, EBP-04). -->

<!-- End of Phase 16 appraise. -->

<!-- Phase 16 qi (EBP-05). -->

<!-- End of Phase 16 qi. -->

<!-- End of Phase 16 tools. -->

## Phase 17: Grants and finance

<!-- Phase 17 tools go below this line. -->

<!-- Phase 17 grants (PLAN-06, PLAN-07, PLAN-08). -->

<!-- End of Phase 17 grants. -->

<!-- Phase 17 budget (PLAN-09, PLAN-10). -->

<!-- End of Phase 17 budget. -->

<!-- Phase 17 finance (LEAD-01, LEAD-02). -->

<!-- End of Phase 17 finance. -->

<!-- End of Phase 17 tools. -->

## Phase 18: Leadership, accreditation, and professional practice

<!-- Phase 18 tools go below this line. -->

<!-- Phase 18 strategy (LEAD-03, LEAD-04, LEAD-05). -->

<!-- End of Phase 18 strategy. -->

<!-- Phase 18 workforce (LEAD-06, LEAD-07, LEAD-08). -->

<!-- End of Phase 18 workforce. -->

<!-- Phase 18 ethics (EBP-07, POL-05, POL-06). -->

<!-- End of Phase 18 ethics. -->

<!-- End of Phase 18 tools. -->

## Phase 19: Policy and law

<!-- Phase 19 tools go below this line. -->

<!-- Phase 19 tracking (POL-01, POL-03). -->

<!-- End of Phase 19 tracking. -->

<!-- Phase 19 law (POL-02, POL-04, POL-07). -->

<!-- End of Phase 19 law. -->

<!-- End of Phase 19 tools. -->
