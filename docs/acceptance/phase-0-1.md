# Acceptance script: Phases 0 and 1

The product owner (the public health nurse) accepts a phase by running these prompts in Claude
Desktop on a **Windows** computer and a **Mac**, using the synthetic corpus in
`fixtures/synthetic/data` copied into the mounted `PHN-Data` folder. Record pass/fail and notes.

## Setup checks

- [ ] `docker run --rm ghcr.io/spiroganas/public-health-nursing-mcp-server:0.2.0 --version` prints the version.
- [ ] The Claude Desktop configuration from the README starts the server; 11 tools appear.
- [ ] Agency IT has been asked whether Docker Desktop, Podman Desktop, or Rancher Desktop is approved (template below).

## Phase 0

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "Set my town to Amherst and our board of health number to 413-555-0100." | `configure_jurisdiction` | County Hampshire, HMCC Region 1, districts listed. |
| 2 | "Set my town to Worchester." | `configure_jurisdiction` | Suggests Worcester; nothing saved. |
| 3 | "Is whooping cough reportable? How fast?" | `lookup_reportable_condition` | Immediate by phone; DPH line; draft caveat. |
| 4 | "A lab called about Salmonella Typhi in a blood culture." | `lookup_reportable_condition` | Typhoid fever (not salmonellosis). |
| 5 | "Do I need to report head lice?" | `lookup_reportable_condition` | Not on the list, but clusters/unusual illness reminder. |
| 6 | "Calculate pertussis rates per 100,000: Amherst 12 cases of 39,263; Hadley 3 of 5,325." | `calculate_rates` | Exact CIs; Hadley flagged unreliable and suppressible. |
| 7 | "Where do your answers come from? Are any sources stale?" | `get_data_source_status` | Lists sources, dates, review status. |

## Phase 1

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 8 | "What files are in my folder?" | `read_local_file` | Lists files. |
| 9 | "Look at line_list.csv. What's in it?" | `read_local_file` or `profile_local_dataset` | Columns and identifier warnings; **no names, DOBs, or phone numbers shown**. |
| 10 | "Profile line_list.csv with value summaries." | `profile_local_dataset` | Counts by town with small cells shown as *; ages over 89 as 90+. |
| 11 | "Make a de-identified copy of line_list.csv I can share." | `deidentify_dataset` | File in output/; report lists dropped/changed columns; rare-combination warning. |
| 12 | "Suppress small cells in counts_by_town.csv." | `apply_small_cell_suppression` | Small counts and complements show *. |
| 13 | "What does our policy say about needlestick injuries?" | `search_agency_policies` | Cites needlestick_exposure.md, § Needlestick and sharps injuries; example SSN redacted. |
| 14 | "Summarize client_note.txt." | `read_local_file` | Strict redaction applied; no client identifiers. |
| 15 | "Write a one-page pertussis fact sheet for parents and export it as Word and PDF with our letterhead." | `export_document` | Files in output/; accessibility self-check passes. |
| 16 | "Create exclusion letters for everyone in exclusion_letters.csv." | `export_document` (mail-merge) | Letters written; the chat never shows the names. |
| 17 | "Is the PDF you just made accessible?" | `check_document_accessibility` | No failures. |

## Template: request to agency IT

> We would like to run the Public Health Nursing MCP Server, an open-source tool that helps AI
> assistants work with public health reference data and our own documents. It runs locally in a
> container (Docker Desktop, Podman Desktop, or Rancher Desktop), reads only the folder we
> choose, has no telemetry, can run fully offline, and calls only public data sources listed by
> the tool. Which container runtime is approved for agency computers, and does our proxy require
> a custom certificate bundle? The source code, SBOM, and signed images are published on GitHub.
