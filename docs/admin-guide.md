# Administrator guide for IT staff

For the IT team at a local health department, board of health, or school district that is
asked to approve and install the Public Health Nursing MCP Server on a nurse's computer.

- **Written:** 2026-11-13, for the code on branch `phase-19-hardening` (workspace version 0.15.0,
  148 tools). **No release has been published yet** (`CHANGELOG.md`), so image names and tags
  below are the ones the release workflow is written to produce, not ones you can pull today.
- **How to read the labels.** Statements were checked against the repository. Where a statement
  could not be verified (because nothing was run in a container for this guide, or the source
  is a third party's documentation), it is marked **not verified**. Please test those on a pilot
  machine before rolling out.
- Nurses have their own plain-language guide (`docs/user-guide/PHN-MCP-Server-User-Guide.docx`).
  The security reasoning is in `docs/security/threat-model.md`; the checks to run before a
  release are in `docs/security/release-checks.md`.

## 1. What this software is, in one paragraph

An MCP server (a "tool provider" for an AI assistant such as Claude Desktop). Claude Desktop
starts it as a Docker container on the nurse's computer and talks to it over standard input and
output; **there is no network service and no listening port**. The container reads files the
nurse puts in one folder, looks up public data on the internet, and writes any generated
document to an output folder. **Everything a tool returns is sent to the AI vendor** as part of
the conversation. Because the agency generally has no business associate agreement with the AI
vendor, the server is built to return counts and summaries instead of client records and to
write anything identifiable to disk instead of returning it. That design reduces risk; it does
not remove it (automated detection of identifiers can miss things). Your agency's own policy on
using AI tools comes first.

## 2. Requirements

| Item | Requirement | Verified |
|---|---|---|
| Computer | Windows or macOS with Docker Desktop, or Linux with Docker Engine. The release workflow builds **linux/amd64 and linux/arm64** images (`.github/workflows/release.yml`), so Apple silicon is supported. | From the workflow |
| Container runtime | Docker Desktop (a paid subscription may be needed in larger organizations), or the free Podman Desktop or Rancher Desktop (`README.md`). Ask your IT approval process which one is allowed. | Podman and Rancher have **not** been tested by this project (the plan lists them as alternatives) |
| MCP client | Claude Desktop, configured through `claude_desktop_config.json` (`README.md`). Other MCP clients that start a stdio command should work: **not verified**. | README |
| Disk | Image size is **not yet measured** (budget: to be decided by the product owner). Bundled data snapshots are about 12 MB (`STATUS.md`); fonts, name lists, and a search engine are embedded in the binary (ADR 0001). The optional full geocoder index is 35.5 MB (ADR 0011). | Not measured |
| Memory | **Not measured.** The server has no limit on input file size (threat model F2), so set a container memory limit (section 9). | Not measured |
| Network | Outbound HTTPS (port 443) to the hosts in section 6, optionally through your proxy. None if you run offline. | Code review |
| Accounts | None required. Free API keys are optional (section 7). | Code review |
| Time zone | The image sets `TZ=America/New_York` (Dockerfile). Dates and deadlines ("today") follow it. | Dockerfile; CI checks it |

## 3. Installing and pulling the image

Image name (from the release workflow): `ghcr.io/spiroganas/public-health-nursing-mcp-server`
(the repository name in lower case). Tags created for a release `vX.Y.Z`: `X.Y.Z` and `X.Y`
(and `latest`, which is the default behavior of the tagging action: **not verified**). **Use the
exact version tag, not `latest` or `X.Y`**, so a restart never changes what the nurses run.

```sh
docker pull ghcr.io/spiroganas/public-health-nursing-mcp-server:X.Y.Z
docker run --rm ghcr.io/spiroganas/public-health-nursing-mcp-server:X.Y.Z --version
docker run --rm ghcr.io/spiroganas/public-health-nursing-mcp-server:X.Y.Z check
```

`check` prints the data folder, cache folder, settings file, offline mode, local time and time
zone, the town, the enabled toolsets, the suppression setting, the content files with their
review status and review-due dates, and `OK` after verifying the SHA-256 of every bundled data
snapshot (`crates/phn-server/src/main.rs`). Keep its output with your approval record.

**Networks that block the registry.** Each release has one tarball per architecture. Download
`phn-mcp-server-vX.Y.Z-amd64.tar.gz` (or `-arm64`) from the GitHub release, then:

```sh
docker load -i phn-mcp-server-vX.Y.Z-amd64.tar.gz
docker tag phn-mcp-server:vX.Y.Z-amd64 ghcr.io/spiroganas/public-health-nursing-mcp-server:X.Y.Z
```

The second command matters: the client configuration uses the registry name, and without the
tag Docker tries to download it (`README.md`).

**Supply-chain checks you can make** (the release workflow produces these; none has run yet, so
the commands are **not verified**):

- The image is signed with a keyless Sigstore signature tied to the workflow run. Verify with
  `cosign verify <image>@<digest> --certificate-identity-regexp '^https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server/.github/workflows/release.yml@refs/tags/v' --certificate-oidc-issuer https://token.actions.githubusercontent.com`.
- The release has `sbom.cdx.json` (a CycloneDX list of every Rust crate in the server, built from
  `Cargo.lock`). It does not list the base image's packages.
- The release job scans the image with Trivy and stops on a fixable critical or high finding.
  One documented exception exists (`.trivyignore`: CVE-2026-84782 in the base image's OpenSSL
  library, which the server does not use because it uses rustls); it expires 2026-12-11.

**Building it yourself:** `docker build -t ghcr.io/spiroganas/public-health-nursing-mcp-server:X.Y.Z .`
from a checkout of the tagged source (Rust is pulled in by the Dockerfile).

**What is in the image:** the server binary, the distroless runtime (glibc, CA certificates,
time zone data; no shell and no package manager), an empty `/data`, and a world-writable `/cache`
(mode 1777) so a named volume copies that mode. It runs as user 65532 by default
(`Dockerfile`).

## 4. The Claude Desktop (MCP) configuration

Open Claude Desktop, then Settings, Developer, Edit Config. The file is:

- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`

(Both from `README.md`. A Linux path is not given there: **not verified**.)

### 4.1 Baseline (from the README)

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
        "ghcr.io/spiroganas/public-health-nursing-mcp-server:X.Y.Z"
      ]
    }
  }
}
```

`-i` keeps stdin open (required: the protocol runs over it). `--rm` removes the container when
the client exits. On Windows use `"C:\\Users\\YOUR-NAME\\PHN-Data:/data"` (double backslashes in
JSON). Restart Claude Desktop after every change. Each tool call must be allowed by the nurse
in the client's own prompt; **do not tell nurses to choose "always allow"** for tools that write
files or change settings.

### 4.2 Hardened variant (recommended for production)

```json
{
  "mcpServers": {
    "public-health-nursing": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "--read-only",
        "--cap-drop", "ALL",
        "--security-opt", "no-new-privileges",
        "--memory", "2g",
        "--pids-limit", "256",
        "-v", "/Users/YOUR-NAME/PHN-Data:/data",
        "-v", "phn-cache:/cache",
        "-e", "PHN_PROFILE=cd_nurse",
        "-e", "PHN_SUPPRESSION_THRESHOLD=5",
        "ghcr.io/spiroganas/public-health-nursing-mcp-server:X.Y.Z"
      ]
    }
  }
}
```

Why each flag is there is in section 9. Add `"-e", "PHN_OFFLINE=1"` and `"--network", "none"`
for a no-internet deployment (section 6).

## 5. Folders, user IDs, and file ownership

| Container path | What it holds | Who writes | Notes |
|---|---|---|---|
| `/data` | The nurse's files. A bind mount of one host folder (for example `PHN-Data`). | The nurse (inputs); the server writes only under `/data/output` | Mount **one dedicated folder**, never a home folder or a drive root: tools can read anything inside it. |
| `/data/output` | Everything the server generates: letters, rosters, de-identified copies, workbooks, charts, matched address files. **May contain identifiable data.** | The server (never overwrites; adds `-2`, `-3`, to a name) | Treat as client data under your retention policy. The server never deletes anything. |
| `/cache` | Settings (`phn-settings.toml`), the HTTP cache (`phn-cache.sqlite`), search indexes (`index/`), temp files (`tmp/`), optional newer snapshots (`snapshots/`), optional data (`geocoder/`, `law_corpus/`). | The server | A named volume (`phn-cache`) is the default. No client files are stored here, but policy text (redacted at indexing) and search terms are. |

Names `/data/output`, `/data/policies`, `/cache/phn-settings.toml` can be changed with
`PHN_OUTPUT_DIR`, `PHN_POLICY_DIR`, `PHN_SETTINGS_FILE` (rarely needed).

### Read-mostly `/data`

You can mount the data folder read-only and give the server one writable output folder:

```
-v /Users/YOUR-NAME/PHN-Data:/data:ro
-v /Users/YOUR-NAME/PHN-Output:/data/output
```

A review of every write site in `crates/phn-tools/src` and `crates/phn-local/src` found that
tools write only to the output folder and to `/cache`, so this should work. **Not verified by
running it.** Create the `output` mount point inside `PHN-Data` first; a read-only parent folder
cannot have it created for you. Also note that the server keeps working if `/data` is read-only
but the output folder is not writable: tools that write files then fail with a message.

### User IDs and ownership

- **Windows and macOS (Docker Desktop):** the runtime maps the container's file access to the
  signed-in user, so files the server creates belong to the nurse (`README.md`). Windows
  permissions on shared drives or synced folders (OneDrive, SharePoint) were **not verified**;
  keep `/data` on a local disk unless your IT team has tested it.
- **Linux:** the container runs as user 65532 unless told otherwise. Add `"--user", "1000:1000"`
  (the output of `id -u` and `id -g`) right after `--read-only` so the server can read and write
  your folder and the output files belong to you (`README.md`). The image makes `/cache` mode
  1777, so a **named volume** works for any user ID, including a `--user` one; CI tests the
  `--user 1234:1234` case (`.github/workflows/ci.yml`).
- A named volume created by version 0.14.0 or earlier is not writable by other user IDs: remove
  it once (`docker volume rm phn-cache`; this deletes saved settings and the download cache,
  which are rebuilt) (`README.md`).
- **Rootless Podman or Docker with user namespaces:** the container user is mapped to a
  different host ID. A `--userns=keep-id` style option is the usual answer: **not verified here**.
- If you use a **bind-mounted host folder** for `/cache` instead of a named volume (easier to
  back up), it must be writable by the container user or by the `--user` ID: **not verified**.

## 6. Network: hosts, proxy, offline

### Hosts the server contacts

Read from the data adapters' URL constants in `crates/phn-data/src` and a few tool files on
2026-11-13. Hosts that appear only as citation links are not listed. The authoritative live list
is the tool `get_data_source_status` (with `check_live: true` it probes each source) plus your
proxy logs from a pilot. Some are blocked on some government networks (`STATUS.md`, "Lessons
learned"); the server falls back to its cache or a bundled snapshot, and every answer says which.

| Host | Used for | Key |
|---|---|---|
| `data.cdc.gov` | PLACES, NNDSS, respiratory and wastewater, overdose, youth survey, maternal and infant data | none |
| `www.cdc.gov`, `tools.cdc.gov`, `stacks.cdc.gov`, `www2a.cdc.gov` (vaccine codes), `dprp.cdc.gov` (Diabetes Prevention Program registry), `www.atsdr.cdc.gov` | CDC guidance and data files | none |
| `api.census.gov` | American Community Survey | optional `PHN_CENSUS_API_KEY` (some networks need it) |
| `eutils.ncbi.nlm.nih.gov` | PubMed search | optional `PHN_NCBI_API_KEY` |
| `api.crossref.org` | DOI metadata (only the DOI is sent) | optional `PHN_CROSSREF_MAILTO` (an email address) |
| `api.fda.gov`, `dailymed.nlm.nih.gov`, `rxnav.nlm.nih.gov` | Drug labels, recalls, shortages, RxNorm | optional `PHN_OPENFDA_API_KEY` |
| `api.weather.gov`, `www.weather.gov`, `www.wpc.ncep.noaa.gov` | Weather alerts and HeatRisk | none |
| `files.airnowtech.org`, `www.airnowapi.org` | Air quality files and API | optional `PHN_AIRNOW_API_KEY` (API only) |
| `www.fema.gov` | OpenFEMA data | none |
| `data.epa.gov`, `eeaonline.eea.state.ma.us` | Drinking water (SDWIS) and PFAS results | none |
| `data.cms.gov`, `data.hrsa.gov`, `empowerprogram.hhs.gov`, `findtreatment.gov`, `www.ers.usda.gov`, `odphp.health.gov` | Nursing homes, health centers, power-dependent population, treatment locator, food access, Healthy People and preventive services | none |
| `www.mass.gov` | DPH documents and data (for example the arbovirus workbook) | none |
| `www.mwra.com` | MWRA wastewater | none |
| `malegislature.gov` | Massachusetts bills (live) | none |
| `www.federalregister.gov`, `api.regulations.gov` | Federal rules and comment dockets | optional `PHN_REGULATIONS_GOV_API_KEY` (for the Regulations.gov cross-check) |
| `api.simpler.grants.gov` | Federal grant search (an HTTPS **POST**) | `PHN_GRANTS_GOV_API_KEY` needed for live results |

The server **never contacts any other host**: no tool takes a URL, host, or endpoint as an
argument (threat model I8). It sends no telemetry. The request's User-Agent is
`public-health-nursing-mcp-server/<version>` with the project's GitHub address. Request URLs
carry the model's search terms (for example a PubMed query). Redirects are followed.

### Proxy and TLS inspection

- `HTTPS_PROXY`, `HTTP_PROXY`, and `NO_PROXY` are honored (the HTTP library's standard behavior,
  `crates/phn-data/src/http.rs`). Pass them with `-e`, for example
  `"-e", "HTTPS_PROXY=http://proxy.example.org:8080"`. Proxy authentication: **not verified**.
- If your proxy inspects TLS, put your CA certificate (PEM, may hold several) in `/cache` or
  `/data` and set `PHN_CA_BUNDLE` to its container path (for example `/data/agency-ca.pem`). It
  is **added** to the system roots; it does not replace them. A missing or invalid file stops the
  server at startup with a message. Certificate verification is never turned off.
- Some API keys travel in the URL query string (Census, NCBI, openFDA, AirNow, Regulations.gov).
  A TLS-inspecting proxy will see them. The server removes keys from every message it returns.

### Offline operation

- `PHN_OFFLINE=1` (also `true`, `yes`, `on`) makes every network call fail immediately and uses
  only bundled snapshots and the cache. Any other value is an error at startup, so a typo cannot
  leave the network on (`crates/phn-core/src/config.rs`).
- `--network none` on the run line removes the network from the container. Use both for a
  strict deployment. With `--network none` alone (offline mode off) each live call waits to fail
  and then falls back, which is slower: **not measured**.
- Tools that need the internet say so in their answers; the user guide marks each tool `no`,
  `optional`, or `yes` for internet use. Weather alerts are never served from a snapshot
  (`STATUS.md`).

## 7. Environment variables and settings

Set with `-e NAME=value` in the client configuration. **Environment values win over the settings
file** (`crates/phn-core/src/config.rs`), so pin anything you do not want the model or the nurse
to change.

### Deployment variables

| Variable | Default | Meaning |
|---|---|---|
| `PHN_DATA_DIR` | `/data` | The mounted data folder. |
| `PHN_CACHE_DIR` | `/cache` | Settings, HTTP cache, indexes. If it is not writable the server starts, warns (on stderr and in `check`), keeps a cache in memory, and cannot save settings. |
| `PHN_OUTPUT_DIR` | `/data/output` | Where generated files go. Created on first use. |
| `PHN_POLICY_DIR` | `/data/policies` | The default folder for `search_agency_policies`. |
| `PHN_SETTINGS_FILE` | `/cache/phn-settings.toml` | The settings file. |
| `PHN_OFFLINE` | off | Section 6. |
| `PHN_CA_BUNDLE` | none | Section 6. |
| `PHN_HTTP_TIMEOUT_SECS` | `20` | Whole-request timeout in seconds. A non-number stops the server at startup. The connect timeout is fixed at 10 seconds. |
| `PHN_LOG` | `info` (set in the image) | Log filter (`error`, `warn`, `info`, `debug`, `trace`, or per-module filters). Logs go to stderr only. **Keep it at `info` or lower**: at `debug` or `trace` the MCP library logs full tool arguments and results (threat model F6). |
| `TZ` | `America/New_York` (set in the image) | Time zone for dates and deadlines. Set another IANA zone for another state. |
| `HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY` | none | Section 6. |

### What the model and the nurse can choose (and what you can pin)

| Variable | Default | Meaning |
|---|---|---|
| `PHN_PROFILE` | none (all tools on) | `school_nurse`, `cd_nurse`, `phn_leader`, or `preparedness`. Hyphens are accepted (`cd-nurse`). |
| `PHN_TOOLSETS` | all | Comma-separated toolsets, or `all`. Overrides the profile. `core` is always on. A list that names no toolset stops the server. |
| `PHN_SUPPRESSION_THRESHOLD` | `5` | Counts from 1 up to the threshold minus one are suppressed in frequency tables. Allowed 2 to 30. **Pin this**: without it, the model can lower it to 2 through `configure_jurisdiction` (threat model F1). |

When `PHN_PROFILE` or `PHN_TOOLSETS` is set, the toolsets are fixed: a nurse who asks for a tool
outside them gets a message that explains the change must be made in the client configuration
and the client restarted.

**Profiles** (`crates/phn-core/src/toolsets.rs`; tool counts from `crates/phn-tools/src/registry/`,
148 tools in all):

| Profile | Toolsets | Tools |
|---|---|---|
| `school_nurse` | core, school, vax, cd, bh, comms, epi | 64 |
| `cd_nurse` | core, cd, vax, epi, comms, equity | 60 |
| `phn_leader` | core, epi, equity, planning, leadership, policy, comms, ebp, chronic | 78 |
| `preparedness` | core, epr, env, epi, equity, comms, cd | 70 |

**All 17 toolsets** and their tool counts: core 9, cd 13, vax 9, school 8, mch 7, aging 7, bh 7,
epr 11, env 8, chronic 7, equity 11, epi 13, planning 11, leadership 8, policy 7, comms 5, ebp 7.
(Toolsets reduce clutter in the model's tool list; they are **not** a security boundary.)

### Optional API keys and contact address

`PHN_CENSUS_API_KEY`, `PHN_NCBI_API_KEY`, `PHN_OPENFDA_API_KEY`, `PHN_AIRNOW_API_KEY`,
`PHN_REGULATIONS_GOV_API_KEY`, `PHN_GRANTS_GOV_API_KEY`, and `PHN_CROSSREF_MAILTO`. Everything
works without them except live federal grant search (needs the Grants.gov key) and the
Regulations.gov cross-check; the Census key is needed for live ACS data from some networks. Free
keys come from each provider. Blank values are ignored.

- Supply them with `-e NAME=value`, or with `--env-file /path/to/phn.env` (a file readable only
  by the nurse or by a service account). **Anyone who can run `docker inspect` on the container
  can read them.** They are low-value public-API keys, but treat them as credentials.
- The server adds a key to the request only. It never writes it to the cache, a citation, a log
  line, an error message, or a tool result; tests cover this (`apifetch.rs`, `grants.rs`,
  `core/medication.rs`). `PHN_CROSSREF_MAILTO` is sent to Crossref only.
- A key in a variable the server does not know (a typo) is silently unused.

### The settings file

`/cache/phn-settings.toml`, written by the `configure_jurisdiction` tool: town, county, HMCC
region, school district and code, board of health phone, suppression threshold, preferred
languages, profile, toolsets, letterhead (agency name, address, phone, fax, email, website, logo
path, footer text), and the vaccine recommendation basis. It holds **no client data**. A
threshold outside 2 to 30 in the file is ignored (the default is used) and reported at startup.
A damaged file produces a warning and default settings. Back it up if nurses spent time on it.

## 8. Optional folders and files

All are optional and read-only to the server. Paths are inside the mounted data folder unless
noted.

| Folder or file | Used by | What it is |
|---|---|---|
| `/data/policies` | `search_agency_policies` | The agency's policies and procedures (PDF, Word, HTML, Markdown, text). Indexed locally; SSNs and labeled birth dates, record numbers are removed at indexing, and a file that looks like client data is redacted strictly. |
| `/data/templates/<language>/` | School, maternal-child, shelter, aging, and CERC tools | Agency-approved translations of letters and plans (ADR 0008). Language folders are named in lower case (`spanish`, `haitian_creole`, or a code such as `es`). The server ships English only and never machine-translates. |
| `/data/local_regulations` | `search_public_health_law` | The board of health's local regulations, searched with the state law. Client-like documents are refused. The `local_folder` argument can name another folder. |
| `/data/law_corpus` or `/cache/law_corpus` | `search_public_health_law` | An optional larger law corpus built by `xtask/scripts/build_law_snapshots.py --extended`. Used only if its manifest format matches and every file matches its SHA-256 (`crates/phn-data/src/law.rs`). |
| `/data/geocoder` or `/cache/geocoder` | `geocode_and_aggregate_addresses` | The optional full offline address index (35.5 MB, all 351 towns) built by `xtask/scripts/build_geocoder_index.py` (ADR 0011). Without it a four-town sample and a ZIP-to-town crosswalk are used. Mount read-only: `-v /path/to/index:/cache/geocoder:ro`. |
| `/data/resources/community_resources.csv` | `find_community_resources` | The agency's local resource directory (`crates/phn-tools/src/equity/resources.rs`). |
| `/data/output` | many | Generated files (section 5). |
| `/cache/snapshots` | all snapshot-backed tools | A newer snapshot set in the same layout, used only when it is newer, lists the same files, and every file matches its checksum (`crates/phn-data/src/snapshots.rs`). **The checksums come from that folder's own manifest, so it protects against corruption, not tampering**: do not let other people write `/cache`. No distribution channel for newer snapshot sets exists yet; new data arrives with new image versions. |
| Files named in tool arguments | Many tools | Rosters, line lists, logs, announcements and so on, anywhere under `/data`. |

## 9. Security hardening settings

Only settings that the code or the Dockerfile justifies are listed. "Run before approval" means
the setting was **not run** for this guide and the release checklist
(`docs/security/release-checks.md`) should exercise it.

| Setting | Why | Status |
|---|---|---|
| Run as non-root (the default `USER 65532`, or `--user <your id>` on Linux) | The Dockerfile sets it; the server never needs root. | Default; CI tests both |
| `--read-only` | The image is built to work with a read-only root: all writes go to `/data` and `/cache` (Dockerfile header comment; CI smoke tests use it). A compromise cannot change the program files. | CI-tested |
| `--cap-drop ALL` | The server needs no capability: no listening port, no process execution, no privileged calls (searched `crates/*/src`: the only `TcpListener` and no `Command` uses are in tests or absent). | **Not run**; check before approval |
| `--security-opt no-new-privileges` | Blocks privilege gain through setuid programs; the distroless image should have none (**not verified**). | **Not run** |
| `--memory 2g --pids-limit 256` | The server reads whole files into memory with no size limit (threat model F2); a limit contains a runaway file. **The numbers are examples, not measurements**: raise `--memory` if large workbooks fail, and replace it with the figure from the release memory check. | Example |
| `--network none` plus `PHN_OFFLINE=1` | For deployments that must not reach the internet; every tool still works from bundled data. | Offline mode CI-tested; `--network none` **not run** |
| Pin `PHN_SUPPRESSION_THRESHOLD` (and `PHN_PROFILE` or `PHN_TOOLSETS`) | So the model cannot change them (threat model F1). | Code-verified |
| Mount one dedicated folder | Tools can read anything inside `/data`. | Policy |
| `/data:ro` with a separate `/data/output` mount | Limits writes to one place. | **Not run** (section 5) |
| Keep `PHN_LOG` at `info` | Debug logging records tool arguments and results (F6). | Code-verified |
| Do **not** mount the Docker socket, use `--privileged`, or add a shell or web-browsing MCP server to the same Claude Desktop profile | A prompt-injected instruction could use them (threat model T1). | Policy |
| Keep Claude Desktop's per-tool approval on | The nurse sees each file-writing and settings call. | Policy |

Things the server does **not** do for you: encrypt `/data` or `/data/output` (use disk
encryption), log who used it, delete old output, or verify the nurse's identity.

## 10. Updating

1. Read the release notes (`CHANGELOG.md` and the GitHub release). Check the new image with the
   `check` command (section 3) and, if you have the checklist, the release checks.
2. `docker pull ...:X.Y.Z` (or load the tarball), then edit the tag in each
   `claude_desktop_config.json` and restart Claude Desktop. The settings and cache volume carry
   over. Remove old images with `docker image prune` when no one needs them.
3. **Content currency.** Every bundled reference file (reportable conditions, disease rules,
   immunization schedules, school requirements, and so on) carries a `review_due` date and a
   status (`pending` until the subject matter expert reviews it). CI fails when a date passes,
   **but a running server does not warn**; `check` and `get_data_source_status` show the dates.
   Plan to replace images at least every six months and after every change in a Massachusetts
   regulation or a CDC or DPH schedule. Today every file is still `pending`, with the first
   `review_due` date 2027-02-01.
4. **Data snapshots** (ACS, SVI, PLACES, MassGIS layers, surveillance, directories) are dated in
   `get_data_source_status` and refreshed by new image versions. The project's weekly
   workflow re-checks the MassGIS snapshots only; the others are refreshed by hand.
5. If an API stops working (federal datasets have disappeared before), tools fall back to the
   cache, then to a snapshot, and say so in the answer.

## 11. Backups

- **Back up `/data` and `/data/output`** under your normal client-record rules. The server keeps
  no other copy of the nurse's files.
- **Settings only matter in `/cache/phn-settings.toml`.** Everything else in `/cache` (HTTP cache,
  indexes) is rebuilt. With a named volume, copy the file out through a stopped container:
  `docker create --name phn-tmp -v phn-cache:/cache <image>`, then
  `docker cp phn-tmp:/cache/phn-settings.toml .`, then `docker rm phn-tmp` (**not run**; the
  image has no shell, so a plain `docker exec` will not work). A bind-mounted cache folder is
  backed up like any other folder.
- Re-creating the settings by hand is a one-minute task for the nurse (ask the assistant to set
  the jurisdiction), so a backup is optional.

## 12. Logging and troubleshooting

**Logs.** The server writes diagnostic lines to stderr (startup settings, warnings such as an
unwritable cache or an out-of-range threshold, cache and indexing problems). Claude Desktop
keeps the server's stderr in its log folder (macOS `~/Library/Logs/Claude`, Windows
`%APPDATA%\Claude\logs`; from Anthropic's documentation, **not verified here**). The server does
not log tool arguments or results at `info`, and has no audit log (threat model S3): the only
record of what was sent to the AI vendor is the nurse's conversation.

**First check:** run `check` on the same machine with the same mounts:

```sh
docker run --rm -v /Users/YOUR-NAME/PHN-Data:/data -v phn-cache:/cache <image> check
```

| Symptom | Likely cause and fix |
|---|---|
| The tools do not appear in Claude Desktop | Docker is not running; the image name in the configuration does not match a local image (Docker tries to download it); JSON syntax error; the client was not restarted. Test with `docker run --rm <image> --version`. |
| "the data folder ... does not exist. Mount a folder at /data" | The `-v host:/data` part is missing or the host folder does not exist. `check` prints `MISSING`. |
| Files are not found | Paths are relative to the data folder (`line_list.csv`, not `PHN-Data/line_list.csv`). Ask the assistant to list the folder. Hidden files (names starting with `.`) are not listed. |
| "path not allowed" | The path has `..`, points outside `/data`, or is a symbolic link that leaves `/data`. This is the sandbox working. |
| Permission denied writing output (Linux) | Add `--user <uid>:<gid>`. Check the folder owner. |
| "cache ... is unavailable ... using an in-memory cache" or settings do not save | `/cache` is not writable. Use a named volume, or remove an old `phn-cache` volume made by 0.14.0 or earlier (section 5). Settings and the policy index then are not kept between sessions. |
| Dates are off by a few hours near midnight | `TZ` is not an Eastern-time zone. Check `local time` in `check`. |
| Live data is missing or old | The host is blocked (section 6), the API needs a key (Census), or offline mode is on. Run `get_data_source_status` with `check_live: true`. The answer states `live`, `cache`, or `snapshot`. |
| "PHN_CA_BUNDLE ... is not a PEM bundle" | The file is not PEM text, or the path is the host path instead of the container path. |
| The server stops when a very large file is opened | Memory limit hit (threat model F2). Split the file or raise `--memory`. |
| A tool says its toolset is not enabled | `PHN_PROFILE` or `PHN_TOOLSETS` in the client configuration fixes the toolsets; edit it and restart. |
| Nothing works after a protocol error | The server's standard output carries only the protocol; if a wrapper script prints to stdout, the client fails. Run the container directly, as above. |

## 13. Data retention in `/cache`

- **HTTP cache** (`phn-cache.sqlite`): public API responses with the request URL (which includes
  search terms the model chose). Bodies older than **90 days** are deleted whenever the server
  starts (`crates/phn-data/src/cache.rs`, `MAX_AGE_DAYS`). The reachability table keeps the latest
  result per source.
- **Search indexes** (`/cache/index/*`): redacted text from the agency's policy, regulation, and
  similar documents. An index is rebuilt completely when a file in its folder changes, **but it
  is not removed when the folder is deleted**, and a deleted file's text stays until that folder
  is searched again (`crates/phn-local/src/policy_index.rs`). After removing a sensitive document
  from `/data/policies`, run one policy search to rebuild, or delete the cache volume.
- **`tmp/`**: a downloaded workbook is written here and removed immediately.
- **Settings**: kept until changed.
- To wipe everything the server keeps outside your data folder: stop Claude Desktop, then
  `docker volume rm phn-cache` (settings are lost and rebuilt).

## 14. Uninstalling

1. Remove the `public-health-nursing` entry from `claude_desktop_config.json`; restart Claude
   Desktop.
2. `docker rmi ghcr.io/spiroganas/public-health-nursing-mcp-server:X.Y.Z` (every tag you pulled),
   and delete any `.tar.gz` you loaded.
3. `docker volume rm phn-cache`.
4. Decide what to do with the data folder. **`/data/output` can contain identifiable data**;
   dispose of it under your records policy. The server did not copy your input files anywhere
   outside `/cache/index` (redacted policy text) and `/data/output`.
5. Remove the Docker runtime if nothing else uses it.

## 15. What to ask the vendor of this software (the project owner)

- A signed, published image and an SBOM (none yet).
- The measured image size, memory use, and startup time (budgets are to be decided by the product
  owner; see `docs/security/release-checks.md` for how to measure).
- Completion of subject matter review of the bundled content before any real-client use.
- Answers to the open items in `docs/security/threat-model.md`, especially F1 and F2.
