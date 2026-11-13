# Threat model review: Public Health Nursing MCP Server

- **Date:** 2026-11-13
- **Scope:** the system as built through Phase 19 (148 tools registered; code at the head of
  branch `phase-19-hardening`, workspace version 0.15.0). No release has been published yet
  (`CHANGELOG.md`).
- **Method:** a read-only review of the source, the Dockerfile, `deny.toml`, and the three
  GitHub workflows. Nothing was built, fuzzed, or run against a real container, so every
  statement below says what the code does, not what an attacker could or could not achieve at
  run time. Where a mitigation is cited, the file (and test) was read; where something could not
  be verified, it says **not verified**.
- **Audience:** the product owner (a DNP-prepared public health nurse), agency IT and privacy
  staff, and whoever picks up hardening work next. Plain language first; file paths and line
  numbers are for the developer.
- **Related:** `docs/adr/0004-privacy-guardrails.md`, `docs/adr/0008-local-fill-templates-and-agency-translations.md`,
  `docs/adr/0001-rust-rmcp-stdio-docker.md`, `docs/admin-guide.md`, `docs/security/release-checks.md`.

## 1. Summary

The central design decision holds up on review: the server does not expose a way to read
row-level local data by default, it has no shell or process execution, it takes no URL from the
model, it writes only through one sandbox, and it makes outbound calls only to fixed public
hosts. The main remaining risks are:

1. The **model is the weakest link**. Anything a tool returns, and anything the model is told to
   do by text inside a document, a bill, a funding announcement, or an API response, is
   untrusted. The server has no defense against an injected instruction beyond limiting what the
   tools can do (see F1).
2. **Input-size limits are missing**, so one very large or booby-trapped file can exhaust memory
   and stop the server (F2).
3. A few **small leaks and gaps** that the peer review did not catch (F3 to F7).
4. **Supply-chain hygiene** is good in some places (lockfile, `cargo deny`, `cargo audit`,
   Trivy, SBOM, signed images) and weak in others (unpinned release action, base images by tag,
   no automated dependency updates) (F8, F9).

The findings are ranked in section 7. None is a "stop the release" item by itself except that
F1 and F2 should be understood by the people who approve use with real client data.

## 2. Assets

| Asset | Where | Why it matters | Sensitivity |
|---|---|---|---|
| Client files (line lists, rosters, notes, letters, intake forms) | The folder mounted at `/data` | Identifiable health information. A leak is a privacy incident. | Highest |
| Generated output (merged letters, de-identified copies, matched address files, workbooks) | `/data/output` (`Paths::output_dir`, `crates/phn-core/src/config.rs`) | Written to hold identifiable data by design (ADR 0004 item 6, ADR 0008). | Highest |
| Agency documents (policies, regulations, board notes, grant material) | `/data/policies`, `/data/templates`, `/data/law_corpus`, `/data/local_regulations`, others | Confidential but not client data; sent to the AI in redacted snippets. | Medium |
| Policy and law search indexes | `/cache/index/*` (`crates/phn-local/src/policy_index.rs`) | Redacted copies of agency text (redaction happens at index time). | Medium |
| HTTP cache | `/cache/phn-cache.sqlite` (`crates/phn-data/src/cache.rs`) | Public API responses plus the request URLs, which contain the model's search terms. Bodies older than 90 days are deleted when the cache opens. | Low to medium |
| Settings | `/cache/phn-settings.toml` (`Config::save_settings`) | Town, school district, phone, letterhead and footer text, suppression threshold, toolsets. Affects what every later answer and document says. | Medium (integrity matters more than secrecy) |
| API keys | Environment variables `PHN_*_API_KEY` in the client's `docker run` arguments | Free-tier keys for public APIs; low value but should not leak into outputs or logs. | Low |
| Bundled reference content and data snapshots | Embedded in the binary (`content/`, `snapshots/`) | Clinical and legal decision support. Wrong or stale content is a patient-safety and legal risk. | Integrity-critical |
| The nurse's trust | Tool output shown as authoritative | A confident wrong answer is the most likely real-world harm. | n/a |

## 3. Actors and trust boundaries

```
 +-------------------------------- nurse's computer --------------------------------+
 |                                                                                    |
 |  [MCP client: Claude Desktop] --stdin/stdout--> [container: phn-mcp-server]        |
 |        |  (B2)                       (B1)             |            |               |
 |        |                                              |            +--> /cache     |
 |        v                                              +--> /data (bind mount, B4) |
 |   AI vendor (cloud)                                   |                            |
 |   sees: the nurse's chat, every tool result           +--> internet (B3) ----------+--> fixed public hosts
 +------------------------------------------------------------------------------------+
```

| Boundary | What crosses it | Controls | Trust assumption |
|---|---|---|---|
| **B1** nurse's machine to container | Docker run arguments, environment variables, bind mounts, stdio | Image runs as non-root (UID 65532), distroless, no shell; `--read-only` is supported and used in CI smoke tests (`.github/workflows/ci.yml`) | The client configuration (the `docker run` line) is trusted. It decides every hardening flag, so a poor snippet weakens everything. |
| **B2** container to MCP client and AI vendor | Every tool result (markdown plus structured JSON); every tool argument the model chooses | Aggregate-first outputs, suppression, redaction, masking, the privacy block in every envelope | **The AI vendor is outside the agency's control and generally has no business associate agreement** (ADR 0004). Everything that crosses B2 must be treated as disclosed. The model is also an untrusted decision-maker: its tool arguments can be steered by text it has read. |
| **B3** container to the internet | HTTPS GET (and one POST) to fixed hosts; API keys; the model's search terms inside query strings | `PHN_OFFLINE`, `--network none`, proxy and CA bundle support, no URL argument, TLS verification on | Public hosts are semi-trusted: they can return wrong, hostile, or unexpectedly large content. |
| **B4** mounted folder | Files an agency or a third party put in the folder (PDFs, spreadsheets, Word files, funding announcements, bills) | Sandbox (`crates/phn-local/src/sandbox.rs`); parsers run in memory only; nothing is executed or extracted to disk | Files in `/data` are **untrusted input**, even though the nurse put them there. Anyone who can write to the folder (sync clients, shared drives, email attachments saved by the nurse) can supply content. |
| **B5** build and release pipeline | Source, dependencies, base images, GitHub Actions | Lockfile, `cargo deny`, `cargo audit`, Trivy, SBOM, cosign signature | The upstream registries and action authors are trusted to varying degrees (section 6). |

Actors: the nurse; a co-worker with access to the same computer or shared folder; an author of
a document the nurse opens in the assistant (a NOFO, a bill, a PDF from a family); a compromised
or malicious public data host; a malicious dependency or action; the AI vendor; and the model
itself acting on injected instructions.

## 4. Data flow summary

1. The nurse asks a question in the MCP client. The client sends it, plus the tool list, to the
   AI vendor. (Outside this server.)
2. The model calls a tool. The client forwards the call over stdio to the container
   (`crates/phn-server/src/server.rs`, `call_tool`).
3. The tool reads: bundled content and snapshots (embedded in the binary); the HTTP cache and
   public APIs (fixed hosts, `crates/phn-data`); and, when given a path, a file under `/data`
   through the sandbox.
4. The tool computes and returns a `phn_core::Envelope` (summary, data, sources, caveats,
   privacy block). **This is the only data path to the vendor.**
5. Tools that create documents write them to `/data/output` (or `/data/output/<subfolder>`) and
   return only file names and counts.
6. Logs go to stderr. Stdout carries the protocol (`crates/phn-server/src/main.rs`).

What can leave the container: (a) tool results over stdio; (b) HTTPS requests to the hosts in
`docs/admin-guide.md` section 6, which carry request URLs that can contain the model's search
terms and, for some APIs, an API key in the query string; (c) files written to the mounted
folder; (d) stderr.

## 5. Threats (STRIDE)

Legend. **Mitigation** lists only what was found by reading the code; "test" names are the
tests that exercise it. **Residual** is what is left after the mitigation. Finding numbers
(F1 to F14) refer to section 7.

### 5.1 Information disclosure (the main category)

| # | Threat | Mitigation found (file, test) | Residual risk | Recommended follow-up |
|---|---|---|---|---|
| I1 | Identifiers returned from a client table (names, DOB, MRN, addresses) | `crates/phn-tools/src/localdata.rs`: `LocalTable::column` refuses identifier columns (except age, date, geography, ZIP, which are aggregated); `categorical` refuses more than 20 levels or values that look like identifiers. `crates/phn-tools/src/core/files.rs`: `mask_row` (identifiers hidden, dates to year, ZIP to three digits, ages over 89 to "90+", long numbers hidden); row samples only with `include_rows`, 200 rows at most. `crates/phn-local/src/table.rs`: `screen_headers` blanks identifier-like or name-like header cells so a file with no header row does not echo its first row. Tests: `no_row_level_data_is_returned_by_default` (about 300 call rows over the synthetic corpus, 2,003 seeded identifiers in `fixtures/synthetic/seeded_identifiers.json`), `requested_rows_are_masked`, `deidentified_file_on_disk_is_clean`, `phi_detector_flags_every_seeded_identifier` (all in `crates/phn-server/tests/mcp_integration.rs`) | Detection is heuristic (name lists, patterns). A column of at most 20 distinct free-text values that no pattern flags (for example a "Provider" or "Notes" column with short entries) is shown as categories. The leak-test loop does not assert that each row succeeded (F11), so a row that errors early proves little. | Assert `is_error == false` per row (or mark expected errors). Add a corpus file with low-cardinality free text. |
| I2 | Identifiers in document text (notes, letters, policies) returned by `read_local_file` or search tools | `core/files.rs` `read_local_file`: standard redaction by default (SSNs, labeled DOB, record and plan numbers) that **escalates to strict** when the text has client signals, a client-identifier table, or a list-of-people density; "there is no opt-out" (comment in the code). `crates/phn-local/src/policy_index.rs` (lines about 250 to 285): text is redacted **before** it is stored in the index, strict for client-like files. `crates/phn-tools/src/policy/law.rs` `client_like_files` (line 728): refuses client-like files in the local regulations folder. Tests: `phi_detector_flags_every_seeded_identifier`, `policy_search_cites_files_and_sections`, `no_row_level_data_is_returned_by_default` rows for `client_note.txt` and the policy file | By design, standard redaction leaves names, addresses, and dates in documents that carry no client signal, because agency policies contain staff names. The `redaction` argument lets the model choose standard or strict; it cannot choose less than standard. `search_agency_policies` accepts a `folder` argument, so the model can point it at any folder in `/data`, including one holding client documents; only the SSN and labeled-DOB rules plus the escalation protect it. | Tell nurses to keep client files out of the folders they ask the assistant to search. Consider refusing client-like files in `search_agency_policies` as `search_public_health_law` does. |
| I3 | A value echoed in an **error message** | Header names are screened (I1). Arguments echoed in errors (for example `crates/phn-tools/src/cd/engine.rs` line 39) are what the model sent. In the places sampled (`school/screenings.rs` lines 773 to 778), problems from a file cell use fixed text and a row number. The peer review fixed most echoes (`STATUS.md` section 2). Known open: aging local-fill, the disparities error, the after-action feedback header summary (`STATUS.md`, "Open after the peer review"). **Panic messages are echoed** (F4). | Not systematic: no test drives every error path with a seeded file and checks the text. The leak test does scan error text (it joins `content` regardless of `is_error`), but only for the calls it makes. | Add an "error fuzz" test: call every path-taking tool with a corrupted copy of each seeded file and scan the error text. Fix F4. |
| I4 | **File names, folder names, sheet names, and absolute paths** disclose a client (for example `Smith_John_vaccines.pdf`) | Sheet names that look like a person's name are shown as `sheet_N` (`table.rs` about line 213). Output file names are sanitized and generic (`sanitize_file_name`, `sandbox.rs` line 142). | **Not mitigated:** `read_local_file` "list" returns every file and folder name under `/data` (`extract.rs` `list_files`; `core/files.rs`); the CSV table name is the file name (`table.rs` `read_delimited`); `PhnError::Parse` carries the absolute path (`crates/phn-core/src/error.rs`); the policy index reports `(file, error)` for files it could not read (`policy_index.rs` line 244). See F7. | Run `phi::scan_text` on names in listings and replace flagged ones with `file_N`; tell nurses to name files by ID, not by client name. |
| I5 | Small counts let someone identify a person | `crates/phn-epi/src/suppression.rs` (primary and complementary suppression) applied by `localdata`-based tools; threshold default 5, allowed 2 to 30 (`config.rs` lines 27 to 33); `PHN_SUPPRESSION_THRESHOLD` pins it. Test: `suppression_hides_small_cells_and_their_complements`. | The model can lower the threshold to 2 through `configure_jurisdiction` (F1). The concussion log's `injuries` total is not suppressed (`STATUS.md`). The model can ask overlapping questions and subtract (differencing); nothing tracks repeated queries. | Pin `PHN_SUPPRESSION_THRESHOLD` (admin guide section 5). Document differencing as a residual risk in the privacy training for nurses. |
| I6 | **API keys** leak into outputs, caches, citations, or errors | Keys are read from the environment only (`crates/phn-data/src/apifetch.rs` `api_key`, line 27), added to the request URL only, never to the cache key or the cited URL (module comment lines 6 to 12), and removed from failure messages by `scrub` (line 205). The grants API sends its key in a header, and the error path scrubs it (`crates/phn-data/src/grants.rs` lines 290 to 302). Tests: `failures_carry_the_live_http_status_without_the_key` and `invalid_cached_bodies_are_not_used_and_snapshot_is` (apifetch.rs), `live_search_sends_the_key_in_a_header_and_never_echoes_it` (grants.rs), the API-key assertion in `crates/phn-tools/src/core/medication.rs` line 637. The HTTP client never sends a `Referer` (`http.rs` line 64). | Keys in query strings (Census, NCBI, openFDA, AirNow, Regulations.gov) are visible to a TLS-inspecting proxy and in its logs. Anyone who can run `docker inspect` on the container sees every `-e` value. Redirects are followed (reqwest default), including to plain HTTP; the key is not re-sent, because the redirect target is a new URL, but this was not tested. | Use an env file readable only by the user; consider Docker secrets; ask the proxy team not to log query strings. |
| I7 | **Logs** contain client data | All logging is `tracing` to stderr (`main.rs` lines 20 to 25). The non-test `tracing::` call sites (about 12; for example `crates/phn-data/src/cache.rs` lines 77 and 147, `crates/phn-tools/src/core/status.rs` line 113) log errors from the cache and the like, not tool arguments or results. No non-test `println!` or `eprintln!` outside `main.rs` (those found are inside `#[cfg(test)]` modules). | With `PHN_LOG=debug` or `trace`, the MCP library itself logs full requests and results (`rmcp-3.9.0/src/service.rs` lines 1568 and 1621), including tool arguments and everything returned. Stderr is normally kept by the MCP client in a log file (that detail is from the client's documentation, not verified here). Default is `info` (Dockerfile). See F6. | Never set `PHN_LOG` above `info` when real data is mounted. Add this to the admin guide (done) and consider ignoring `debug` and `trace` in `main.rs`. |
| I8 | **Exfiltration through the server's own network calls** (model puts local data in a search term) | No tool takes a URL. A scan of every `*Args` and `*Input` struct in `crates/phn-tools/src` found only path-type arguments to `/data` files, no URL, host, or endpoint argument. URLs are constants plus percent-encoded identifiers (`crates/phn-data/src/socrata.rs` `encode`, `soql_string`; `eutils.rs` `url`; bill numbers are normalized in `malegislature.rs` `fetch_bill`). Crossref receives only a DOI (`crossref.rs` header comment). | Free-text search arguments (PubMed queries, legislation keywords, medication names, grant searches) go to government hosts and are written to the cache. The model could place data it has seen into such a term. Low bandwidth, goes only to the allow-listed hosts, and the model already has the data. | Egress allow-list at the agency proxy; tell nurses that search terms leave the machine. |
| I9 | Disclosure through **hyperlinks or images in generated documents** | `crates/phn-docs/src/docx.rs` (line 91) writes hyperlinks only for `http`, `https`, and `mailto:` targets; every text and attribute is XML-escaped (`esc`, line 21). Images come only from `/data` through the sandbox (`core/export.rs` line 173) and must be PNG or JPEG with alt text. | A link in a letter the model drafted can point anywhere (this is also how phishing works). Not fetched by the server. | Nurse review before sending; the admin guide says so. |

### 5.2 Tampering and elevation (prompt injection, file writes, parsers)

| # | Threat | Mitigation found | Residual risk | Recommended follow-up |
|---|---|---|---|---|
| T1 | **Prompt injection** through text the tools return: documents in `/data` (policies, notes, NOFOs, PDFs, transcripts), live API text (PubMed abstracts, Federal Register abstracts, bill titles and history, NWS alerts, openFDA label text, nursing-home and treatment-locator records), bundled corpora | Tool results are data, but **the server does not mark or filter them**: a search of the code for "untrusted" and "prompt injection" found nothing, and `INSTRUCTIONS` (`server.rs` lines 25 to 33) do not warn the model. HTML is stripped from some API text (`fedreg.rs` `strip_tags`). What the server does limit is the **damage** an injected instruction can do, in order: (1) no shell or process execution anywhere in `crates/*/src` (no `Command::new`; `std::process` only for `process::id()`); (2) no URL argument (I8); (3) all writes go through `Sandbox::new_output_file` or `school/templates.rs` `write_in` into `/data/output` or `/cache` (a scan of every `fs::write`, `File::create`, `fs::copy`, and `create_dir_all` in `crates/phn-tools/src` found no other target); (4) files are numbered, not overwritten (`sandbox.rs` line 71; see F5 for the race); (5) the container root is read-only when the client uses `--read-only`; (6) the server has no listening port. | An injected instruction can still: make the model read any file in `/data` (the model can already), write arbitrary content or links into `/data/output` with `export_document`, **change settings with `configure_jurisdiction`** (lower suppression to 2, enable every toolset, persist letterhead footer text into every later document) (F1), and send chosen search strings to the public hosts (I8). Exfiltration beyond the AI vendor depends on **other tools in the same MCP client** (web fetch, other servers), which this server cannot control. | (a) Add a sentence to `INSTRUCTIONS` that text inside tool results, documents, and web content is data, not instructions. (b) Pin `PHN_SUPPRESSION_THRESHOLD`, `PHN_PROFILE` or `PHN_TOOLSETS` in the client configuration (the settings in the environment win over the file, `config.rs` line 385). (c) Keep the client's per-tool approval prompts on; do not choose "always allow" for write tools. (d) Do not install a general web-browsing or shell MCP server in the same client as this one when real client data is mounted. (e) Consider an `untrusted_text: true` marker in the envelope for results that quote external or local text. |
| T2 | **Path traversal and symlink escape** | `crates/phn-local/src/sandbox.rs`: `join_checked` (line 104) rejects any `..` component and absolute paths outside `/data` or the canonical root; `resolve_existing` (line 44) canonicalizes (follows symlinks) and `ensure_inside` (line 129) requires the result to stay inside the canonical data root; `list_files` uses `follow_links(false)` (`extract.rs`). Tests: `sandbox::tests::blocks_traversal_absolute_and_symlink_escape`, `resolves_relative_and_container_paths`; server-level `bad_input_is_a_tool_error_with_a_helpful_message` (`../../etc/passwd`, `/etc/passwd`). Every file argument in `crates/phn-tools/src` goes through `resolve_existing` (spot-checked: `core/files.rs`, `comms/accessibility.rs`, `mch/content.rs`, `core/configure.rs` for the logo, `core/export.rs` for images and merge data). | A symlink **inside** `/data` pointing to another place inside `/data` is allowed (intended). The sandbox is application-level: a bug in one tool that opens a path without the sandbox would escape it, and the container's own read-only root and mounts are the second layer. The write side has a gap (F5). | Mount only a dedicated folder, never a home directory. Keep `--read-only`. Add a test that walks every path-taking tool with `../` and an escaping symlink (the leak test already enumerates the tools). |
| T3 | **Writes outside `/data/output`, overwrite of existing files** | As T1 point 3 and 4; chart and export subfolders are created under `output_dir()`. `school/templates.rs` `write_in` uses `create_new(true)`. | `Sandbox::new_output_file` checks `exists()` and later writes with `fs::write`: a dangling symlink in the output folder, or a file created between the check and the write, is followed or overwritten (F5). `Sandbox::output_dir` canonicalizes `PHN_OUTPUT_DIR` but does not require it to be inside `/data` (admin-controlled, so acceptable). | Create files with `OpenOptions::create_new(true)` everywhere (one helper in `sandbox.rs`). |
| T4 | **Malicious Word, Excel, PDF, HTML, or CSV file** (parser bugs, decompression bombs) | Parsers are pure Rust libraries (`zip` 9.4.1 with deflate only, `quick-xml` 0.42, `calamine` 0.38, `lopdf` 0.47, `scraper` 0.29, `csv` 1.4); `unsafe_code = "forbid"` in the workspace lints and every crate opts in (`Cargo.toml`, each crate's `[lints]`). Nothing is executed or extracted to disk: DOCX members are read by name into memory (`docx.rs` `read_member`); no macros are run; HTML is parsed, not rendered; PDF actions are not followed. PDF parsing is wrapped in `catch_unwind` (`pdf.rs` lines 49 and 60). Row cap of 1,000,000 (`table.rs` line 11). | **No size limits** on file bytes (`std::fs::read` in `table.rs:126`, `docx.rs:140`, `pdf.rs:43`, `extract.rs:147,164`), on a zip member (`docx.rs:168` `read_to_string` with no limit: a DOCX with a small compressed part that expands to gigabytes), on PDF page count, or on spreadsheet cell counts; no per-tool timeout (the only timeout is the HTTP one, `http.rs:59`). A panic inside a tool that runs on the async runtime (not through `blocking`) may stop the server: **not verified**. See F2. | See F2. Also run the parsers on a malformed-file corpus (cargo-fuzz or a small mutation script) before v1.0. |
| T5 | **Formula injection** in generated spreadsheets and CSV | XLSX: user and model text is written with `write_string` (`crates/phn-docs/src/xlsx.rs` lines 135, 148, 195, 197), which stores a string cell, never a formula; numbers are written as numbers only when `as_number` accepts them. The only formulas in the code base (`planning/budget/workbook.rs`, `planning/performance.rs`) are built from cell references and numbers, not from user text. CSV: `crates/phn-docs/src/csv_out.rs` `neutralize` prefixes a leading apostrophe to cells starting with `=`, `+`, `-`, `@`, tab, or carriage return unless the cell is a plain number; used by `table_to_csv`, which is used by de-identification, suppression, geocoding, and forecast file output. Test: `formula_like_cells_are_written_as_text`. | **Gap:** `save_watchlist` in `crates/phn-tools/src/policy/legislation.rs` (lines 1367 to 1441) writes a CSV with a raw `csv::Writer`. Its cells include model-chosen keywords and topics (lines 758 and 905), and remote text (bill title, last action text, committee). A value beginning with `=`, `+`, `-`, or `@` becomes a formula when the nurse opens the file in Excel. Reported as F3. | Use `phn_docs::csv_out::table_to_csv` there, and strip a leading apostrophe when `watchlist_path` is read back. |
| T6 | **Spoofed or tampered settings** (persistent injection) | Settings are written atomically (`config.rs` `save_settings`); an out-of-range threshold in the file is ignored (`config.rs` line 385; test `out_of_range_threshold_in_the_settings_file_is_ignored`). Environment variables win over the file. | The model can write the file through `configure_jurisdiction` (annotated `idempotent`, `destructive: false`) (F1). `/cache` is mode 1777 in the image (Dockerfile), so another local account sharing the volume could edit the file: **not verified for a real multi-user host**. | As F1. |

### 5.3 Spoofing, repudiation, denial of service

| # | Threat | Mitigation found | Residual risk | Recommended follow-up |
|---|---|---|---|---|
| S1 | **Spoofed data host or poisoned data**: a man in the middle or a compromised public host returns altered data | TLS through `rustls` with the platform verifier (`crates/phn-data/Cargo.toml`: reqwest features `rustls`, `http2`); no `danger_accept_invalid_*` anywhere (grep); extra roots only through `PHN_CA_BUNDLE`, merged not replaced (`http.rs`, `tls_certs_merge`). Bodies are validated before they are cached or used (`apifetch.rs` `expect_json`, `expect_xml`, per-adapter validators), so an HTML block page is not cached. Every answer states `live`, `cache`, or `snapshot` mode and its date. | A host can serve plausible but wrong JSON; the server cannot tell. Responses are not size-limited (T4, F2). The SQLite cache is unsigned, so anyone who can write `/cache` can plant cached bodies (they would be shown as `cache` mode). | Agency proxy allow-list. Keep answers' source and date visible to the nurse (already in every envelope). |
| S2 | **Snapshot override planted in `/cache`** | `SnapshotStore::valid_override` (`crates/phn-data/src/snapshots.rs` line 602) accepts a newer snapshot only if the manifest entry is newer, lists the same files, and each file matches its checksum. Test: `newer_valid_override_wins_and_bad_checksum_is_ignored`. | The checksums come **from the same folder**, so the check protects against corruption, not against a person who can write `/cache`: they can supply a newer manifest and matching files. The same pattern is used for `law_corpus/` (`crates/phn-data/src/law.rs`) and the geocoder index (ADR 0011). Rated F10. | Document that `/cache` must not be writable by other people; for shared hosts use a private per-user volume. Optionally sign manifests. |
| S3 | **No audit trail** (repudiation): the agency cannot show what was sent to the vendor | Every envelope has a privacy block (`row_level_data`, `suppression_applied`, `files_written`, `identifiers_detected`), visible in the conversation. | The server keeps no log of tool calls by design (no PHI in logs). The only record is the client's conversation history. | If the agency needs an audit log, add an opt-in log of tool name, time, and privacy block only (no arguments). Product decision. |
| D1 | **Denial of service** by large input (huge CSV, PDF, DOCX, spreadsheet, or HTTP body) | `MAX_ROWS = 1_000_000` (`table.rs`); SVG-to-PNG size cap (`svg.rs`, about line 190); HTTP timeouts of 20 seconds total and 10 seconds to connect (`http.rs`); path listing limited to 500 entries and depth 8 (`extract.rs`). Container memory limit if the client adds one. | No byte limits (T4, F2). The policy index rebuilds **completely** whenever any file in the folder changes (`policy_index.rs` line 166 onward; `STATUS.md`). No per-tool deadline. The HTTP body is read fully into memory (`http.rs` lines 125 and 169). | F2; set `--memory` and `--pids-limit` (admin guide section 9). |
| D2 | A tool **panic** | `blocking()` runs file-heavy tools in `spawn_blocking`, so a panic becomes a tool error (`server.rs` lines 97 to 108). | The error text includes the panic message (F4). Some wrappers call tool code directly on the async runtime (`STATUS.md` lists Phases 9, 11, 12, and CERC): whether a panic there ends the server was **not verified**. | Move those wrappers to `blocking()`; add a test that forces a panic and checks the reply. |

### 5.4 Supply chain, container, integrity, and legal

| # | Threat | Mitigation found | Residual risk | Recommended follow-up |
|---|---|---|---|---|
| C1 | **Malicious or vulnerable Rust dependency** | `Cargo.lock` is committed (512 packages); the Dockerfile and CI use `--locked`. `deny.toml`: licenses allow-list, `wildcards = "deny"`, `unknown-registry` and `unknown-git` denied (crates.io only), `yanked = "warn"`, `ignore = []`. CI runs `cargo deny` (`EmbarkStudios/cargo-deny-action@v2`) and a separate `cargo audit` job. No `openssl` or `native-tls` in `Cargo.lock`; `rusqlite` bundles SQLite and `aws-lc-rs` ships C code (both are compiled from crates.io sources). | There is no Dependabot or Renovate configuration (`.github/` has only `workflows/`), so updates depend on a person. Duplicate versions are only a warning. | Add `.github/dependabot.yml` for cargo, github-actions, and docker; add a `SECURITY.md` with a reporting address. |
| C2 | **Compromised GitHub Action in the release job** | Workflows declare `permissions` (CI: `contents: read`). Release actions are pinned to major tags. | `release.yml` runs `aquasecurity/trivy-action@master` (deliberately unpinned, comment in the file) in a job with `contents: write`, `packages: write`, and `id-token: write`. Other actions are tags, not commit SHAs (`actions/checkout@v5`, `docker/*@v3/v5/v6`, `sigstore/cosign-installer@v3`, `softprops/action-gh-release@v2`, `anchore/sbom-action@v0`, `taiki-e/install-action@v2`, `dtolnay/rust-toolchain@stable` (a branch), `Swatinem/rust-cache@v2`). `taiki-e/install-action` installs an unpinned `cargo-audit`. F8. | Pin every action to a commit SHA; move Trivy to a pinned SHA in a job without write permissions, then publish in a second job. |
| C3 | **Base image and build tool drift** | Runtime is `gcr.io/distroless/cc-debian12:nonroot` (glibc, CA certificates, tzdata, no shell, no package manager); build uses `rust:1.97-bookworm` and `cargo install cargo-chef --locked`. Release scans the image with Trivy (CRITICAL and HIGH with a fix), builds an SBOM from `Cargo.lock`, and signs the digest with keyless cosign. | Base images are referenced by tag, not digest; `cargo-chef` has no version. `.trivyignore` holds one exception, CVE-2026-84782, that **expires 2026-12-11**; a release after that date fails until the base image is updated. The SBOM lists crates, not the base image's packages (acknowledged in `release.yml`). F9. | Pin by digest; pin `cargo-chef@x.y.z`; calendar the 2026-12-11 expiry. |
| C4 | **Container escape, privilege on the host** | Non-root `USER 65532:65532`; no shell; no setuid binaries in distroless (**not verified by image inspection**); the server opens no listening socket (the only `TcpListener::bind` calls are in test modules) and spawns no processes, so `--cap-drop ALL` and `--security-opt no-new-privileges` are not expected to break it (**not run**). CI exercises `--read-only` and `--user 1234:1234`. | The README's client snippet uses `--read-only` but not `--cap-drop`, `--security-opt`, `--network none`, memory or process limits. The Docker run line is chosen by whoever edits the client config. A Linux user running with `--user $(id -u)` gives the container that user's file rights over the mounted folder, as intended. | Offer a hardened snippet (admin guide section 9); add a CI smoke test that runs with `--cap-drop ALL --security-opt no-new-privileges --network none` and `PHN_OFFLINE=1`. |
| C5 | **Tampered or stale bundled content** | Content and snapshots are embedded in the binary (`include_str!`), so the image ships exactly what CI tested. `cargo xtask validate-content` (CI job `content`) checks: JSON Schema for each YAML file, embedded copy equals the file on disk, a source URL exists, `review_due` is a date and **not in the past** (fails CI), every snapshot file against `snapshots/manifest.json` SHA-256, and registry links. `phn-mcp-server check` re-verifies bundled snapshots (`verify_bundled`). Test: `bundled_checksums_match_manifest`. `snapshots.yml` rebuilds the **MassGIS** snapshots weekly and fails on drift. All content is `status: pending`; every answer says so. | The checksum manifest sits in the same repository as the files, so a bad change to both passes (review is the control). The validator does **not** reject `status: reviewed` with an empty `reviewed_by` (`xtask/src/content.rs`; per `STATUS.md` it also misses unregistered YAML files and `meta.id` mismatches, not re-verified). The weekly workflow does not run `build-public-snapshots --check` (ACS, SVI, PLACES) or the surveillance builders. **The running server never warns that content is overdue for review**: the date gate is a CI gate, so an old image in the field keeps answering. The first date gate fires on 2027-02-01. | Add `reviewed_by` and `reviewed_on` checks; add the public-data check to the weekly workflow; have `get_data_source_status` flag content past its `review_due`; tell IT to retire images older than six months or the next release. |
| C6 | **Clinical or legal error treated as advice** | `DECISION_SUPPORT_NOTICE` (`crates/phn-core/src/envelope.rs` line 13) and `.decision_support()` on clinical and legal answers; sources with dates and mode on every answer; review status in caveats; the server submits nothing to any external system (the only POST is the read-only grants search, `http.rs` `post_json_bytes`); the `INSTRUCTIONS` tell the model the nurse decides. Subject matter review is tracked per file. | All 111 content files are unreviewed drafts (`status: pending`); no phase has met its definition of done (`STATUS.md`). The project plan lists a regression set of nurse prompts as planned and not built. A model can still paraphrase a timeline wrongly. | Complete the content review before any real-client use; build the prompt regression set before v1.0. |
| C7 | **Licensing and copyright of bundled material** | `LICENSE` is MIT. Dependencies are limited by `deny.toml` to permissive licenses plus MPL-2.0 and CDLA-Permissive-2.0. DejaVu fonts are embedded under the Bitstream Vera license (`crates/phn-docs/fonts/LICENSE_DEJAVU`). Content records license terms per source and paraphrases copyrighted frameworks rather than copying them (`content/policy/public_health_ethics.yaml` copyright notes; `content/ebp/appraisal_checklists.yaml` licence per checklist; `content/bh/postvention.yaml` license line). The plan keeps licensed material out (Appendix A: CPT, APHA manual, Red Book, ANA standards, ASQ, Beers). | The image carries only the binary (`Dockerfile`): **no third-party license notices** are shipped with it, although MIT, Apache-2.0, and the Bitstream Vera font terms ask for them in binary distributions. Several content files carry CC BY-NC or "fair use" caveats that the product owner must accept (`appraisal_checklists.yaml`, marked unverified in places). The reproduction of statute and regulation text (M.G.L., CMR) is public record but unreviewed by counsel. | Generate a third-party notices file (`cargo about` or `cargo deny list`) and ship it in the image and as a release asset; have agency counsel read the content license notes before wide distribution. |

## 6. What was checked and found sound

- No `unsafe` code (`unsafe_code = "forbid"`, workspace lints in `Cargo.toml`; every crate has `[lints] workspace = true`).
- No process execution and no listening sockets in non-test code.
- No tool argument carries a URL, host, or endpoint.
- Every outbound fetch URL in the adapters is `https` (the two `http://` strings in `crossref.rs` are DOI prefixes to strip; one in `epr/alerts.rs` is a citation link).
- All writes go to `/data/output` or `/cache` (`/cache/index`, `/cache/tmp`, `/cache/snapshots`, the settings file, and the SQLite cache).
- MCP resources are an exact-match lookup in the bundled list (`crates/phn-server/src/resources.rs` `read`); there is no path to the filesystem.
- Prompt arguments are length-limited to 300 characters (`prompts.rs` `check_lengths`).

## 7. Findings, ranked

Severity is for a deployment with real client data mounted. "Fix" is the smallest change that
closes the gap; none was made in this task, which changes documentation only.

### Medium

**F1. The model can weaken privacy settings, and persist text into future documents.**
`configure_jurisdiction` (`crates/phn-tools/src/core/configure.rs` lines 159 to 161 and 191 to
211; wrapper `crates/phn-server/src/server.rs`) is callable by the model without any extra
confirmation. It can set `suppression_threshold` to 2 (the minimum, `config.rs` line 31), change
toolsets, and save a letterhead (agency name, footer text) that is then printed on every
generated document. The file lives in `/cache/phn-settings.toml` and survives restarts. With an
injected instruction in a document, this is a persistent change, not a one-time one.
*Fix (no code):* pin `PHN_SUPPRESSION_THRESHOLD` (and `PHN_PROFILE`) in the client configuration;
the environment value wins (`config.rs` line 385). *Fix (code):* refuse model-initiated
threshold values below the default of 5 unless the environment pin allows it; add the
"untrusted text" sentence to `INSTRUCTIONS`; show the saved letterhead footer in the tool answer
so the nurse sees a change.

**F2. No size limits on files, zip members, PDFs, or HTTP bodies.**
Files are read whole (`crates/phn-local/src/table.rs:126`, `docx.rs:140`, `pdf.rs:43`,
`extract.rs:147` and `:164`); a DOCX member is read with `read_to_string` and no cap
(`docx.rs:168`); an HTTP body is read with `resp.bytes()` (`crates/phn-data/src/http.rs:125`
and `:169`) and stored in SQLite (`cache.rs` `put`). *Input:* a few-megabyte DOCX or XLSX whose
XML part expands to gigabytes, or a very large CSV or PDF in `/data`, or a hostile response from
a public host. *Effect:* memory exhaustion; the container is killed; the MCP client shows the
server as disconnected; no data is lost, but a nurse mid-task loses the session. *Fix:* check
`metadata().len()` before each `fs::read` (for example 50 MB), wrap zip members in
`Read::take(limit)` after checking `ZipFile::size()`, stop reading an HTTP body past a cap
(`Content-Length` and a streamed counter), and set `--memory` in the run line.

### Low

**F3. CSV formula injection in the legislation watchlist.** *Status: fixed on 2026-11-13 (`save_watchlist` neutralizes cells with `phn_docs::csv_out::neutralize_cell`, and the reader strips the leading apostrophe; test `saved_watchlists_never_hold_cells_a_spreadsheet_would_run_as_formulas`).*
`save_watchlist`, `crates/phn-tools/src/policy/legislation.rs:1367-1441`, writes with a plain
`csv::Writer`, bypassing `phn_docs::csv_out::neutralize`. *Input:* `keywords: ["=HYPERLINK(...)"]`
(model-chosen, lines 758 and 905), or a bill title or last-action text from the Legislature that
starts with `=`, `+`, `-`, or `@`. *Effect:* Excel evaluates the cell when the nurse opens
`ma_legislation_watchlist.csv` (a prompt, a link that leaks the sheet, or other formula
behavior). *Fix:* build a `TableBlock` and call `phn_docs::csv_out::table_to_csv`; when the file
is read back through `watchlist_path`, strip one leading apostrophe.

**F4. Panic text is returned to the model.**
`blocking()` (`crates/phn-server/src/server.rs:103-108`) formats the join error. In tokio 1.54.1
that text is `task N panicked with message "..."` (`tokio-1.54.1/src/runtime/task/error.rs:143`).
A Rust panic message can contain the data involved (a string slice error quotes the string; an
`unwrap` on a parse error quotes the input). *Effect:* a client value reaches the vendor. *Fix:*
return a fixed message, and log only a message count on stderr.

**F5. `Sandbox::new_output_file` is check-then-write.** *Status: the dangling-symbolic-link case is fixed on 2026-11-13 (`symlink_metadata` instead of `exists`; test `a_dangling_symlink_in_the_output_folder_is_never_written_through`). The time-of-check race between two writers remains.*
`crates/phn-local/src/sandbox.rs:71-95` tests `exists()` and returns a path; callers then call
`std::fs::write` (for example `policy/legislation.rs:1441`). `exists()` follows symlinks, so a
dangling symlink named like the target looks free and the write follows it; a file created in
between is overwritten. The helper in `school/templates.rs:395-420` (`write_in`) does it
correctly with `create_new(true)`. *Input:* a symlink or file placed in `/data/output` by
another process or user with write access to the mounted folder (a sync client, a shared drive).
*Effect:* a write outside the intended name, bounded by what the container can write (the
mounts). *Fix:* one helper that opens with `OpenOptions::new().write(true).create_new(true)` and
retries the numbered name on `AlreadyExists`; use it everywhere.

**F6. `PHN_LOG=debug` or `trace` logs full tool arguments and results.**
`rmcp-3.9.0/src/service.rs:1568` (`?request`) and `:1621` (`?result`). *Effect:* client data in
stderr and then in the client's log files. Default is `info`. *Fix:* in `main.rs`, cap the filter
for the `rmcp` target at `info`, or document (done in the admin guide) that `PHN_LOG` must stay
at `info` or lower on machines with real data.

**F7. File and folder names are returned unscreened.**
Listing returns every name (`crates/phn-local/src/extract.rs` `list_files`, shown by
`core/files.rs`); CSV table names are file names (`table.rs` `read_delimited`); parse errors carry
absolute paths (`PhnError::Parse`); `policy_index.rs:244` reports names and error text. *Effect:*
`Smith_John_2025.pdf` reaches the vendor. *Fix:* screen names with `phi::scan_text` and show
`file_N` for flagged names; advise ID-based file names.

**F8. Unpinned release action and tag-only pins.** See C2. *Fix:* pin by SHA; split scan and
publish into separate jobs; Dependabot for actions.

**F9. Time-bound release blockers.** `.trivyignore` expires 2026-12-11 (a release after that
fails the scan unless the distroless base is updated); content `review_due` 2027-02-01 turns CI
red. *Fix:* calendar both; review the base image the week before the first release.

**F10. Snapshot, law corpus, and geocoder overrides are self-attesting.** See S2. Applies only
to a person who can write `/cache`, `/data/law_corpus`, or `/data/geocoder`.

**F11. The leak test does not assert success per row.** The loop in
`crates/phn-server/tests/mcp_integration.rs` (`no_row_level_data_is_returned_by_default`, ends
before line 2417) calls each tool and scans the text, but never checks `is_error`. A row whose
arguments are wrong passes trivially. (The schema check that every argument name exists was added
by the peer review and is sound.) *Fix:* add `expect_error: bool` per row.

### Informational

**F12. API keys travel in query strings** (Census, NCBI, openFDA, AirNow, Regulations.gov);
visible to TLS-inspecting proxies and in `docker inspect`. See I6.

**F13. No third-party license notices in the image or release assets.** See C7.

**F14. `status: reviewed` with no reviewer passes `validate-content`.** See C5. Also the runtime
never flags overdue content; only CI does.

## 8. Not verified

- Behavior of a running container (read-only root, `--cap-drop ALL`, `--security-opt
  no-new-privileges`, `--network none`, read-only `/data` with a separate writable
  `/data/output`). Static review says they should work; CI exercises only `--read-only` and
  `--user`.
- Whether a panic in an async wrapper that does not use `blocking()` ends the server (D2).
- Whether Claude Desktop or other clients persist the server's stderr to disk (I7); the claim
  comes from the client vendor's documentation.
- Whether the distroless image contains any setuid binary.
- Behavior of reqwest redirects with respect to an HTTP downgrade (I6).
- The completeness of the "no echo of file values in errors" claim (I3): sampled, not exhaustive.
- The three validator gaps beyond `reviewed_by` that `STATUS.md` lists (C5).
- Anything about the AI vendor's retention, training, or contract terms; that is an agency
  decision (Appendix A, "Privacy" row in the project plan: a BAA is the unlock).
