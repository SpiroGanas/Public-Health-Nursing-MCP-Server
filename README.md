# Public Health Nursing MCP Server

An [MCP](https://modelcontextprotocol.io) server that makes AI assistants such as Claude more
useful to public health nurses working for a Massachusetts city or town, a school district, or
the Massachusetts Department of Public Health (DPH).

It runs in a Docker container on the nurse's own computer. An MCP client such as Claude Desktop
starts it over stdio. Tools work with public reference data, Massachusetts reference lists, and
the nurse's own files in a folder mounted into the container. **Files never leave the
container**, tools return aggregates by default, and anything identifiable is written to an
output folder instead of being sent to the AI model.

> **Status: v0.2.0 — Phases 0 and 1 of the [project plan](project_plan.md).** 11 of the 148
> planned tools are available. The bundled 105 CMR 300.000 reportable-condition list is a
> **draft pending review** by the subject matter expert; every answer from it says so.
> All clinical and legal output is decision support only.

## Tools

| Tool | Plan ID | Toolset | What it does |
|---|---|---|---|
| `get_data_source_status` | CORE-08 | core | Lists every data source with its data date, review status, live reachability, and caveats. |
| `configure_jurisdiction` | CORE-09 | core | Sets the nurse's town (county and HMCC region are derived), school district, board of health phone, suppression threshold, languages, profile/toolsets, and letterhead. |
| `read_local_file` | CORE-04 | core | Lists files in `/data` and extracts text and tables from PDF, DOCX, XLSX, CSV, HTML, Markdown, and text, with identifier warnings and redaction. |
| `search_agency_policies` | CORE-05 | core | Answers "what does our policy say?" from the agency's own policies, with file and section citations. |
| `export_document` | CORE-06 | core | Turns Markdown into accessible DOCX or tagged PDF with letterhead, XLSX, CSV, SVG, or PNG; includes local mail-merge. |
| `calculate_rates` | EPI-01 | epi | Crude and category-specific rates with exact Poisson confidence intervals and small-number flags. |
| `apply_small_cell_suppression` | EPI-06 | epi | DPH-style small-cell suppression with complementary suppression. |
| `profile_local_dataset` | EPI-07 | epi | Describes a local dataset (types, missingness, possible identifiers) without showing identifier values. |
| `deidentify_dataset` | EPI-08 | epi | Writes a HIPAA Safe Harbor de-identified copy (or a limited data set) and reports what changed. |
| `lookup_reportable_condition` | CD-01 | cd | Whether a disease or lab result is reportable in Massachusetts, by whom, how fast, and to whom. |
| `check_document_accessibility` | COMM-05 | comms | Checks DOCX, HTML, and PDF files against Section 508 / WCAG 2.1 AA. |

See [docs/tools.md](docs/tools.md) for arguments, outputs, and privacy behavior of each tool.
Reference content and data snapshots are also exposed as MCP resources (`phn://content/…`,
`phn://snapshots/…`).

## Quick start

### 1. Install a container runtime

Install [Docker Desktop](https://www.docker.com/products/docker-desktop/) on Windows or macOS.
Docker Desktop needs a paid subscription in larger organizations, which can include state and
municipal agencies; [Podman Desktop](https://podman-desktop.io) and
[Rancher Desktop](https://rancherdesktop.io) are free alternatives. Ask your agency's IT
department which runtime is approved before installing.

### 2. Get the image

Once a release is published:

```sh
docker pull ghcr.io/spiroganas/public-health-nursing-mcp-server:0.2.0
```

On networks that block GHCR, download `phn-mcp-server-<version>-amd64.tar.gz` (or `-arm64` for
Apple silicon) from the GitHub release and run `docker load -i <file>`.

To build it yourself from this repository:

```sh
docker build -t ghcr.io/spiroganas/public-health-nursing-mcp-server:0.2.0 .
```

### 3. Create your data folder

Create a folder such as `PHN-Data` in your home folder. Put files you want the assistant to use
in it, and your agency's policies and procedures in `PHN-Data/policies`. Generated documents
appear in `PHN-Data/output`.

### 4. Connect Claude Desktop

Open Claude Desktop → Settings → Developer → Edit Config, and add the server.

**macOS** (`~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "public-health-nursing": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm", "--read-only",
        "-v", "/Users/YOUR-NAME/PHN-Data:/data",
        "-v", "phn-cache:/cache",
        "-e", "PHN_PROFILE=cd_nurse",
        "ghcr.io/spiroganas/public-health-nursing-mcp-server:0.2.0"
      ]
    }
  }
}
```

**Windows** (`%APPDATA%\Claude\claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "public-health-nursing": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm", "--read-only",
        "-v", "C:\\Users\\YOUR-NAME\\PHN-Data:/data",
        "-v", "phn-cache:/cache",
        "-e", "PHN_PROFILE=school_nurse",
        "ghcr.io/spiroganas/public-health-nursing-mcp-server:0.2.0"
      ]
    }
  }
}
```

On **Linux**, add `"--user", "1000:1000"` (your own user and group IDs from `id -u` and
`id -g`) after `--read-only` so the server can write to your folder and the files it creates
belong to you. Docker Desktop on Windows and macOS handles this automatically.

Restart Claude Desktop. The tools appear under the tools (hammer) icon. Start with:

> Set my jurisdiction to Amherst, and my board of health phone to 413-555-0100.

Then try, for example:

> Is a positive HBsAg reportable, and how fast?
> What does our policy say about needlestick injuries?
> Profile PHN-Data/line_list.csv and tell me if it contains identifiers.
> Make a de-identified copy of line_list.csv I can share with the school.

`docker run --rm ghcr.io/spiroganas/public-health-nursing-mcp-server:0.2.0 check` prints the
configuration and verifies the bundled data, which helps when troubleshooting with IT.

## Configuration

Environment variables (set with `-e` in the client configuration) take precedence over the
settings saved by `configure_jurisdiction` in `/cache/phn-settings.toml`.

| Variable | Default | Purpose |
|---|---|---|
| `PHN_PROFILE` | none | Role profile: `school_nurse`, `cd_nurse`, `phn_leader`, or `preparedness`. |
| `PHN_TOOLSETS` | all | Comma-separated toolsets, e.g. `cd,epi`. Overrides the profile. `core` is always on. |
| `PHN_SUPPRESSION_THRESHOLD` | `5` | Counts from 1 to threshold − 1 are suppressed (2–30). |
| `PHN_OFFLINE` | off | `1` disables all network calls; only bundled and cached data are used. |
| `PHN_CA_BUNDLE` | none | PEM file of extra certificate authorities, for agency networks that inspect TLS. Put it in `/cache` or `/data`. |
| `HTTPS_PROXY`, `NO_PROXY` | none | Outbound proxy, if your agency requires one. |
| `PHN_HTTP_TIMEOUT_SECS` | `20` | Timeout for public API calls. |
| `PHN_DATA_DIR`, `PHN_CACHE_DIR`, `PHN_OUTPUT_DIR`, `PHN_POLICY_DIR`, `PHN_SETTINGS_FILE` | `/data`, `/cache`, `/data/output`, `/data/policies`, `/cache/phn-settings.toml` | Folder locations (rarely changed). |
| `PHN_LOG` | `info` | Log level. Logs go to stderr; stdout carries the MCP protocol. |

**Profiles and toolsets.** Exposing every tool at once crowds the model's context, so tools are
grouped into toolsets (`core`, `cd`, `vax`, `school`, `mch`, `aging`, `bh`, `epr`, `env`,
`chronic`, `equity`, `epi`, `planning`, `leadership`, `policy`, `comms`, `ebp`). Profiles enable
a sensible set:

| Profile | Toolsets |
|---|---|
| `school_nurse` | core, school, vax, cd, bh, comms, epi |
| `cd_nurse` | core, cd, vax, epi, comms, equity |
| `phn_leader` | core, epi, equity, planning, leadership, policy, comms, ebp, chronic |
| `preparedness` | core, epr, env, epi, equity, comms, cd |

Changing the profile or toolsets with `configure_jurisdiction` refreshes the tool list
immediately in clients that support tool-list notifications.

## Privacy and safety design

- **Sandbox.** Tools read only inside `/data` and write only to `/data/output`; `..`,
  absolute paths outside `/data`, and symbolic links that escape are rejected.
- **Aggregate first.** Data tools return structure, counts, and statistics, never rows, unless
  `include_rows` is set; even then identifier columns are masked, dates reduced to year, ZIP
  codes to three digits, and ages over 89 shown as 90+.
- **PHI detection.** Column names, value patterns, and public name lists (U.S. Census surnames,
  SSA first names) flag the 18 HIPAA identifiers. Document text is redacted before it is
  returned; a document that contains client identifiers is redacted strictly.
- **Small-cell suppression** is applied automatically to every frequency table.
- **Local mail-merge.** Letters and rosters are written to `/data/output` and never returned.
- **No telemetry.** The server makes no outbound calls except to the public data sources listed
  by `get_data_source_status`, and runs fully offline with `PHN_OFFLINE=1` or
  `docker run --network none`.
- **Decision support only.** Clinical and legal answers cite their sources and say how current
  they are. No tool submits anything to an external system.

Automated detection can miss identifiers. Review outputs before sharing them, and follow your
agency's policy on using AI tools.

## Development

Requirements: Rust 1.88 or later (stable). Then:

```sh
cargo test --workspace                 # unit, golden, and MCP integration tests
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p phn-server --bin phn-mcp-server -- check
cargo xtask validate-content           # schemas, review dates, snapshot checksums
cargo xtask build-snapshots --check    # rebuild MassGIS snapshots and diff them
cargo xtask build-name-lists           # rebuild the PHI detector's name lists
```

The [MCP Inspector](https://github.com/modelcontextprotocol/inspector) is handy for manual
testing: `npx @modelcontextprotocol/inspector cargo run -q -p phn-server --bin phn-mcp-server`
(set `PHN_DATA_DIR` and `PHN_CACHE_DIR` to local folders first).

Repository layout:

```
crates/
  phn-server   # binary: rmcp stdio server, toolset filtering, MCP resources
  phn-core     # configuration, errors, response envelope, toolsets
  phn-data     # HTTP client, SQLite cache, snapshot store, Massachusetts reference lists
  phn-content  # reviewed reference content (YAML + JSON Schema)
  phn-epi      # rates and exact intervals, small-cell suppression
  phn-local    # /data sandbox, parsers, PHI detection, de-identification, policy index
  phn-docs     # DOCX/PDF/XLSX/CSV/SVG/PNG output, mail-merge, accessibility checks
  phn-tools    # tool implementations
content/       # reference content with citations and review metadata
snapshots/     # dated data snapshots + manifest (built by xtask)
fixtures/      # synthetic test corpus (all fictional)
docs/          # tool reference, architecture decisions, acceptance script
xtask/         # snapshot builder, name-list builder, content validator
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to add a tool and how content review works, and
[CHANGELOG.md](CHANGELOG.md) for release notes. [STATUS.md](STATUS.md) is the handoff document
(what is done, what is open, lessons learned), and [CLAUDE.md](CLAUDE.md) holds the standing
instructions for AI coding agents.

## License

MIT. Bundled DejaVu fonts are under the Bitstream Vera license (see
`crates/phn-docs/fonts/LICENSE_DEJAVU`). MassGIS data are public records of the Commonwealth of
Massachusetts. Census and SSA name lists are public domain.
