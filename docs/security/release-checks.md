# v1.0 release checks

A checklist for the coordinator to run before the first tagged release (and again for every
later release). Every command was written by reading the repository on 2026-11-13; the two
Python scripts (sections 5 and 7) were run against an older debug build of the server and
worked. **The Docker, `cargo deny`, `cargo audit`, and Trivy commands were not run when this was
written** (no builds were allowed); expect to correct a flag or two the first time.

- Project facts used here: the CI jobs `lint`, `test`, `content`, `deny`, `audit`, and `docker`
  (`.github/workflows/ci.yml`) and the release workflow (`.github/workflows/release.yml`).
- Related: `docs/security/threat-model.md` (what the checks are protecting) and
  `docs/admin-guide.md` (what IT will ask).

**Budgets.** The project plan, `STATUS.md`, and the ADRs were searched on 2026-11-13 for image
size, memory, and startup-time targets (words such as "MB", "seconds", "startup", "image size").
**None is stated.** ADR 0011 only says the default image and `snapshots/` "may grow by about
10 MB at most" for the offline geocoder sample, and `STATUS.md` notes that the bundled snapshots
are about 12 MB and says to "check image size early". So:

| Measure | Budget |
|---|---|
| Compressed image size (per architecture) | to be decided by the product owner |
| Uncompressed image size | to be decided by the product owner |
| Server memory (peak, normal use) | to be decided by the product owner |
| Time to answer `initialize` | to be decided by the product owner |
| Time to answer `tools/list` | to be decided by the product owner |
| Size of the `tools/list` reply (all tools, and each profile) | to be decided by the product owner |

Record the measurements in section 9 even though there is no budget, so the first release
becomes the baseline for the second.

## 0. Before you start

- [ ] On a clean checkout of the commit to be tagged: `git status` clean; `git rev-parse HEAD` written in section 9.
- [ ] Version agreed in `Cargo.toml` (`workspace.package.version`), `CHANGELOG.md`, `README.md`, and the image tag `vX.Y.Z`. (At the time of writing the workspace version is 0.15.0 although the code has Phases 0 to 19.)
- [ ] Tools installed: Rust stable and the repository's pinned toolchain needs (`rustup update stable`), `cargo-deny` (`cargo install cargo-deny --locked`), `cargo-audit` (`cargo install cargo-audit --locked`), Docker with Buildx, Trivy, Python 3, and (after publishing) `cosign`.
- [ ] Disk: a release build and an image build each need several GB.

## 1. The standard gates

From `CLAUDE.md`. All must pass with no skipped, ignored, or weakened tests.

```sh
cargo fmt --all --check
cargo +stable clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo xtask validate-content
```

- [ ] All four pass. (CI runs them as `lint`, `test`, and `content`.)
- [ ] `cargo xtask validate-content` prints no `FAIL`. It fails by itself when any `review_due` is in the past; the first date is **2027-02-01**. Note the `warn` lines: every file is `PENDING` subject matter review. Decide and record whether v1.0 ships with pending content (the project's rules say only the subject matter expert may mark content `reviewed`).
- [ ] `cargo xtask build-snapshots --check` and `cargo xtask build-public-snapshots --check` (need network) show no unexplained drift. The weekly workflow (`.github/workflows/snapshots.yml`) runs only the first.

## 2. Dependency scans

`deny.toml` allows permissive licenses (plus MPL-2.0 and CDLA-Permissive-2.0), denies wildcard
versions, unknown registries and git sources, and warns on yanked crates and duplicate versions.
`ignore = []` for advisories: there are no accepted exceptions to review.

```sh
cargo deny check                 # advisories, bans, licenses, sources (CI: job "deny")
cargo audit                      # RustSec database against Cargo.lock (CI: job "audit")
cargo tree -d | head -50         # duplicate versions, for information
```

- [ ] `cargo deny check` exits 0. Read the warnings (`multiple-versions`, `yanked`); record any new ones.
- [ ] `cargo audit` reports no vulnerabilities and no unmaintained or unsound crates that matter. If it does, record the advisory id, whether the code path is reachable, and the decision. **Do not add an `ignore` entry without a written reason and an expiry date**, as `.trivyignore` does.
- [ ] No `openssl`, `openssl-sys`, or `native-tls` in `Cargo.lock` (the Trivy exception relies on it): `grep -nE '^name = "(openssl|openssl-sys|native-tls)"' Cargo.lock` prints nothing.
- [ ] If you changed dependencies since the last release: license check result recorded, and `cargo deny check` was run after the change (`CLAUDE.md`).

## 3. The privacy leak test

The leak test is `no_row_level_data_is_returned_by_default` in
`crates/phn-server/tests/mcp_integration.rs`. It calls about 300 argument sets over the
synthetic corpus in `fixtures/synthetic/data` and fails if any of the 2,003 seeded identifiers
in `fixtures/synthetic/seeded_identifiers.json` appears in a result (text or structured). Related
tests in the same file: `phi_detector_flags_every_seeded_identifier`,
`requested_rows_are_masked`, `deidentified_file_on_disk_is_clean`,
`suppression_hides_small_cells_and_their_complements`, and
`bad_input_is_a_tool_error_with_a_helpful_message` (sandbox refusals).

```sh
cargo test -p phn-server --test mcp_integration --locked -- \
  no_row_level_data_is_returned_by_default phi_detector_flags_every_seeded_identifier \
  requested_rows_are_masked deidentified_file_on_disk_is_clean \
  suppression_hides_small_cells_and_their_complements bad_input_is_a_tool_error_with_a_helpful_message \
  exported_documents_pass_the_servers_own_accessibility_check
cargo test --workspace --locked          # the per-phase tests in crates/phn-server/tests/phase*_tools.rs
```

- [ ] These tests pass.
- [ ] **Every tool that reads a file is in the leak test.** `CLAUDE.md` requires it. This approximate script lists tools whose argument struct has a path, file, or folder field and whose name does not appear in the leak test. It is a prompt for a human, not a gate: on 2026-11-13 it printed seven names, all false positives (argument names that merely contain "file" or "path", such as `file_name`, `write_files`, `pathogen`, `profile`). Read each new name it prints.

```sh
python3 -I - <<'EOF'
import re, glob
t = open('crates/phn-server/tests/mcp_integration.rs').read()
start = t.find('async fn no_row_level_data_is_returned_by_default')
leak = t[start:t.find('async fn tools_that_await_a_fetch', start)]
names = {}
for f in glob.glob('crates/phn-server/src/*.rs'):
    s = open(f).read()
    for m in re.finditer(r'#\[tool\(\s*name\s*=\s*"(\w+)"(.*?)\)\]\s*(?:pub\s+)?async fn \w+\s*\(\s*&self,\s*Parameters\(args\):\s*Parameters<([\w:]+)>', s, re.S):
        names[m.group(1)] = m.group(3).split('::')[-1]
pathy = set()
for f in glob.glob('crates/phn-tools/src/**/*.rs', recursive=True):
    s = open(f).read()
    for m in re.finditer(r'pub struct (\w+)\s*\{(.*?)\n\}', s, re.S):
        if re.search(r'pub \w*(path|file|folder)\w*:', m.group(2)):
            pathy.add(m.group(1))
print(len(names), 'tools;')
for n, ty in sorted(names.items()):
    if ty in pathy and '"%s"' % n not in leak:
        print('not in the leak test:', n)
EOF
```

- [ ] **Known gap (threat model F11):** the loop does not assert that each call succeeded. Until that is fixed, spot-check by running the test with `-- --nocapture` after temporarily printing `r.is_error` for rows you changed. Do not leave the temporary change in.
- [ ] **Error-path spot check** (threat model I3). With a data folder containing `fixtures/synthetic/data/line_list.csv` renamed to `Garcia_Maria_line_list.csv`, call `read_local_file` with no path and confirm the name appears (known gap F7), then record whether it still does.
- [ ] No identifiable value is written to stderr at the default log level. Start the binary by hand with a copy of the synthetic corpus as its data folder and stderr saved to a file, send it a few file-tool calls (`read_local_file`, `profile_local_dataset`, `export_document` with `merge_data_path`) through the MCP Inspector, then `grep -E 'Garcia|Maria|[0-9]{3}-[0-9]{2}-[0-9]{4}' stderr.txt` (use names and numbers from `seeded_identifiers.json`). Expect no match. Repeat once with `PHN_LOG=debug` and record that the debug level **does** log arguments and results (threat model F6); that is why the admin guide says to keep `PHN_LOG` at `info`.
- [ ] Sandbox: `cargo test -p phn-local sandbox` passes (`blocks_traversal_absolute_and_symlink_escape`, `output_files_never_overwrite`, `resolves_relative_and_container_paths`).
- [ ] API keys never appear in output: `cargo test -p phn-data apifetch` and `cargo test -p phn-data grants` and `cargo test -p phn-tools medication` pass (they assert the key is absent from errors, citations, and results).

### Negative tests for oversize input (threat model F2)

Until size limits exist this documents the current behavior; after they exist it must be an
error message, not a dead server.

```sh
python3 -I - <<'EOF'
import zipfile, os
os.makedirs('/tmp/phn-bomb/data', exist_ok=True)
with zipfile.ZipFile('/tmp/phn-bomb/data/bomb.docx', 'w', zipfile.ZIP_DEFLATED) as z:
    z.writestr('[Content_Types].xml', '<Types/>')
    with z.open('word/document.xml', 'w', force_zip64=True) as f:
        f.write(b'<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>')
        chunk = b'<w:p><w:r><w:t>aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa</w:t></w:r></w:p>' * 1000
        for _ in range(10000):            # about 700 MB expanded, a few MB compressed
            f.write(chunk)
        f.write(b'</w:body></w:document>')
EOF
```

Then call `read_local_file` on `bomb.docx` against the image run with `--memory 512m`
(section 4). Pass: a tool error and the server still answers `tools/list` afterwards. Fail
(today's expected result): the container is killed.

## 4. Container image: build, size, scans, hardened run

```sh
export IMAGE=phn-mcp-server:release-check
docker build --build-arg VERSION=vX.Y.Z -t "$IMAGE" .
docker image inspect "$IMAGE" --format 'uncompressed bytes: {{.Size}}  arch: {{.Architecture}}'
docker save "$IMAGE" | gzip -9 | wc -c          # compressed bytes (about what a pull transfers)
docker history --no-trunc --format '{{.Size}}\t{{.CreatedBy}}' "$IMAGE" | head -20
docker run --rm "$IMAGE" --version
```

- [ ] Build succeeds from a clean cache (`docker build --no-cache` once per release).
- [ ] Sizes recorded in section 9 (budget: to be decided by the product owner). Also build the other architecture the release will publish (`docker buildx build --platform linux/arm64 ...`) and record its size.
- [ ] The layer list shows the binary layer and the small distroless layers only; no build tools, no source tree.
- [ ] **Image contents:** `docker run --rm --entrypoint /bin/sh "$IMAGE"` fails (there is no shell). If a shell appears, the base image changed.
- [ ] **Scan** (the release workflow does this on the amd64 image; run it first so a failure does not block the tag):

```sh
trivy image --severity CRITICAL,HIGH --ignore-unfixed --ignorefile .trivyignore --exit-code 1 "$IMAGE"
```

  - [ ] Exit 0. `.trivyignore` has one entry, CVE-2026-84782, which **expires 2026-12-11**: if the release is on or after that date and the base image still ships the old `libssl3`, the scan fails. Check whether `gcr.io/distroless/cc-debian12:nonroot` has a fixed `libssl3`, and delete the entry if so.
  - [ ] Any new finding: record id, package, fixed version, and decision.
- [ ] **Smoke tests from CI** (`docker` job in `.github/workflows/ci.yml`), run locally:

```sh
D=$(mktemp -d); mkdir -p "$D/data"
docker run --rm --read-only -e PHN_OFFLINE=1 -v "$D/data:/data" "$IMAGE" check | tee "$D/check.txt"
grep -Eq 'local time: +[0-9-]+ [0-9:]+ -0[45]:00 \(TZ=America/New_York\)' "$D/check.txt" && echo "time zone OK"
```

- [ ] `check` ends with `OK`, lists every content file with its review status and due date, and shows the Eastern time zone.
- [ ] **Hardened run** (admin guide section 4.2). These flags are recommended in the admin guide but were **not run** when it was written; this is the check that proves them:

```sh
python3 -I startup_check.py 3 -- docker run -i --rm --read-only --cap-drop ALL \
  --security-opt no-new-privileges --network none --memory 2g --pids-limit 256 \
  -e PHN_OFFLINE=1 -v "$D/data:/data" -v phn-release-cache:/cache "$IMAGE"
python3 -I startup_check.py 3 -- docker run -i --rm --read-only --user 1234:1234 \
  -e PHN_OFFLINE=1 -v phn-release-cache:/cache "$IMAGE"
```

  (`startup_check.py` is in section 5.) Pass: three runs each, no error, tool count as expected.
  - [ ] Settings save under `--user` with a named volume (the CI "Settings saved" check):

```sh
printf '%s\n' \
 '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ci","version":"0"}}}' \
 '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
 '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"configure_jurisdiction","arguments":{"town":"Amherst"}}}' \
 | timeout 30 docker run -i --rm --read-only --user 1234:1234 -e PHN_OFFLINE=1 \
   -v phn-release-cache:/cache "$IMAGE" | grep -q 'Settings saved' && echo "settings saved"
```

  - [ ] Read-only data, writable output (admin guide section 5; **not yet verified**): mount `-v "$D/data:/data:ro" -v "$D/out:/data/output"` (create `$D/data/output` and `$D/out` first), call `export_document`, and confirm the file appears in `$D/out` and nothing else is written under `$D/data`.
  - [ ] Peak memory (Linux): `printf` the three handshake lines as above into `/usr/bin/time -v target/release/phn-mcp-server > /dev/null`, and read "Maximum resident set size". Record it (budget: to be decided by the product owner).
- [ ] After publishing (the release workflow's output): the image digest is recorded; `cosign verify` (admin guide section 3) succeeds; the release has `sbom.cdx.json` and both `phn-mcp-server-vX.Y.Z-<arch>.tar.gz` files; `docker load` of one tarball plus the `docker tag` command in the README works on a clean machine.

## 5. Startup time

What to measure: how long from starting the process until it answers `initialize`, and until it
answers `tools/list`, over stdio, the way Claude Desktop starts it. The script mirrors the CI
smoke test and the handshake in `crates/phn-server/tests/stdio_binary.rs` (protocol version
`2025-06-18`, then `notifications/initialized`, then `tools/list`). It uses a throwaway data
and cache folder and `PHN_OFFLINE=1`, so no network time is included. It also prints the size
of the `tools/list` reply, because every tool's description and schema is context the model
must read: the full tool list was **1.7 MB** (146 tools) on the older debug build used to test
the script, and 0.6 MB for the `cd_nurse` profile (60 tools). Measure it for each profile.

Save as `startup_check.py`:

```python
#!/usr/bin/env python3
"""Time the MCP handshake of phn-mcp-server over stdio.

usage: startup_check.py RUNS -- COMMAND [ARGS...]

Starts COMMAND RUNS times, sends initialize, notifications/initialized, and tools/list (the
same messages as the CI smoke test and crates/phn-server/tests/stdio_binary.rs), and prints
when each answer arrived, measured from process start. The process environment gets a
throwaway PHN_DATA_DIR and PHN_CACHE_DIR, PHN_OFFLINE=1, and PHN_LOG=warn, which is right for
a binary. For `docker run`, pass the same settings with -e (see the checklist).
"""
import json, os, statistics, subprocess, sys, tempfile, time

if "--" not in sys.argv:
    sys.exit(__doc__)
split = sys.argv.index("--")
runs = int(sys.argv[1]) if split > 1 else 5
cmd = sys.argv[split + 1:]

INIT = {"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                   "clientInfo": {"name": "release-check", "version": "0"}}}
INITIALIZED = {"jsonrpc": "2.0", "method": "notifications/initialized"}
LIST = {"jsonrpc": "2.0", "id": 2, "method": "tools/list"}


def read_response(proc, want_id, deadline):
    while time.monotonic() < deadline:
        line = proc.stdout.readline()
        if not line:
            raise RuntimeError("server closed stdout; stderr: "
                               + proc.stderr.read().decode(errors="replace")[:500])
        try:
            msg = json.loads(line)
        except ValueError:
            raise RuntimeError("non-JSON on stdout (stdout must carry only the protocol): %r" % line[:200])
        if msg.get("id") == want_id:
            return msg, len(line)
    raise RuntimeError("timed out waiting for id %s" % want_id)


def one_run():
    with tempfile.TemporaryDirectory() as tmp:
        env = dict(os.environ, PHN_DATA_DIR=os.path.join(tmp, "data"),
                   PHN_CACHE_DIR=os.path.join(tmp, "cache"), PHN_OFFLINE="1", PHN_LOG="warn")
        os.makedirs(env["PHN_DATA_DIR"])
        t0 = time.monotonic()
        proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, env=env)
        try:
            send = lambda m: (proc.stdin.write((json.dumps(m) + "\n").encode()), proc.stdin.flush())
            send(INIT)
            init, _ = read_response(proc, 1, t0 + 120)
            t_init = time.monotonic() - t0
            send(INITIALIZED)
            send(LIST)
            lst, size = read_response(proc, 2, t0 + 120)
            t_list = time.monotonic() - t0
            return t_init, t_list, len(lst["result"]["tools"]), size, init["result"]["serverInfo"]
        finally:
            proc.stdin.close()
            try:
                proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                proc.kill()


results = [one_run() for _ in range(runs)]
inits, lists = [r[0] for r in results], [r[1] for r in results]
print("server:", results[0][4])
scope = os.environ.get("PHN_TOOLSETS") or os.environ.get("PHN_PROFILE") or "all toolsets"
print("tools listed: %d   tools/list reply: %d bytes   (%s)" % (results[0][2], results[0][3], scope))
print("initialize answered after  median %.3f s  max %.3f s  (%d runs)" % (statistics.median(inits), max(inits), runs))
print("tools/list answered after  median %.3f s  max %.3f s" % (statistics.median(lists), max(lists)))
```

Run it (build `--release` first for a real number; a debug build is several times slower):

```sh
cargo build --release --locked -p phn-server --bin phn-mcp-server
python3 -I startup_check.py 5 -- target/release/phn-mcp-server
PHN_PROFILE=school_nurse python3 -I startup_check.py 5 -- target/release/phn-mcp-server
PHN_PROFILE=cd_nurse     python3 -I startup_check.py 5 -- target/release/phn-mcp-server
PHN_PROFILE=phn_leader   python3 -I startup_check.py 5 -- target/release/phn-mcp-server
PHN_PROFILE=preparedness python3 -I startup_check.py 5 -- target/release/phn-mcp-server
# The same through the container (the first run after a pull is the "cold" number):
python3 -I startup_check.py 5 -- docker run -i --rm --read-only -e PHN_OFFLINE=1 "$IMAGE"
```

On an older **debug** build (a different checkout, 146 tools) the script reported `initialize`
at about 0.08 s and `tools/list` at about 0.12 to 0.18 s. That is for orientation only, not a
release number.

- [ ] Medians and maxima recorded for the binary and the container (budget: to be decided by the product owner).
- [ ] The script exits without "non-JSON on stdout": nothing but protocol messages reaches stdout.
- [ ] The tool count equals the registry (148 at the time of writing: `grep -h '#\[tool(' crates/phn-server/src/*.rs | wc -l`).
- [ ] The `tools/list` byte counts per profile are recorded.

## 6. Time zone and date sanity (from the Dockerfile and the peer review)

- [ ] `TZ` shows `America/New_York` in `check` output, and `-e TZ=America/Chicago` changes it.

## 7. Accessibility of generated documents

`check_document_accessibility` (tool COMM-05, `crates/phn-tools/src/comms/accessibility.rs`;
checks in `crates/phn-docs/src/accessibility.rs`) tests a DOCX, HTML, or PDF file in `/data`
against WCAG 2.1 AA, which Section 508 incorporates. It reports passed, failed, warning, and
**manual** checks (reading order, color alone, sensible alt text always need a person). It does
not read XLSX, CSV, SVG, or PNG files; `export_document` self-checks DOCX and PDF output
(`crates/phn-tools/src/core/export.rs`). PDF headings and table headers are manual checks
(`STATUS.md`).

How the existing tests use it:
- `exported_documents_pass_the_servers_own_accessibility_check` (`mcp_integration.rs`) exports DOCX and PDF with a letterhead and asserts `failed == 0` and `warnings == 0` for both.
- The same call on a tool's own output appears in `phase11_tools.rs` (lines about 453 and 810), `phase13_ics_tools.rs` (169), `phase15_cha_tools.rs` (779), `phase16_qi_tools.rs` (218), `phase18_strategy_tools.rs` (551), and `phase19_tracking_tools.rs` (332). The Phase 19 law and policy-brief tools have no such assertion.
- The check on the planted bad document `fixtures/synthetic/data/client_letter.html` in the leak test confirms the checker finds real problems (heading order) without quoting identifiers.

```sh
cargo test --workspace --locked accessibility
cargo test -p phn-docs accessibility
```

- [ ] These pass.

**Sweep every generated document.** Run the acceptance scripts (`docs/acceptance/phase-*.md`) or
exercise each document-producing tool against the synthetic corpus in a scratch data folder, then
run this over the output folder. It exits 1 when any file fails or warns. Save as
`a11y_sweep.py`:

```python
#!/usr/bin/env python3
"""Run check_document_accessibility on every DOCX, PDF, and HTML file under a data folder.

usage: a11y_sweep.py PATH_TO_BINARY DATA_FOLDER [SUBFOLDER]

DATA_FOLDER is the folder that the server sees as /data (for example the PHN-Data folder
after running the acceptance scripts). SUBFOLDER defaults to "output". The server is started
with a throwaway cache and PHN_OFFLINE=1. Exit status 1 when any file fails or warns.
"""
import json, os, subprocess, sys, tempfile

binary, data = sys.argv[1], os.path.abspath(sys.argv[2])
sub = sys.argv[3] if len(sys.argv) > 3 else "output"
files = []
for root, _dirs, names in os.walk(os.path.join(data, sub)):
    for n in sorted(names):
        if n.lower().endswith((".docx", ".pdf", ".html", ".htm")):
            files.append(os.path.relpath(os.path.join(root, n), data).replace(os.sep, "/"))

with tempfile.TemporaryDirectory() as cache:
    env = dict(os.environ, PHN_DATA_DIR=data, PHN_CACHE_DIR=cache, PHN_OFFLINE="1", PHN_LOG="warn")
    p = subprocess.Popen([binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=env, text=True)

    def rpc(msg):
        p.stdin.write(json.dumps(msg) + "\n")
        p.stdin.flush()
        if "id" in msg:
            while True:
                reply = json.loads(p.stdout.readline())
                if reply.get("id") == msg["id"]:
                    return reply

    rpc({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "a11y-sweep", "version": "0"}}})
    rpc({"jsonrpc": "2.0", "method": "notifications/initialized"})
    bad = 0
    for i, f in enumerate(files, start=2):
        r = rpc({"jsonrpc": "2.0", "id": i, "method": "tools/call", "params": {
            "name": "check_document_accessibility", "arguments": {"path": f}}})["result"]
        if r.get("isError"):
            print("ERROR   ", f, r["content"][0]["text"][:120])
            bad += 1
            continue
        rep = r["structuredContent"]["data"]["report"]
        flag = "ok" if rep["failed"] == 0 and rep["warnings"] == 0 else "ATTENTION"
        if flag != "ok":
            bad += 1
        print("%-9s %s  passed=%d failed=%d warnings=%d manual=%d" % (
            flag, f, rep["passed"], rep["failed"], rep["warnings"], rep["manual"]))
        for c in rep["checks"]:
            if c["status"] in ("fail", "warning"):
                print("          -", c["status"], c["id"], c["message"][:100])
    p.stdin.close()
    p.wait(timeout=10)
print("%d file(s) checked, %d need attention" % (len(files), bad))
sys.exit(1 if bad else 0)
```

```sh
rm -rf /tmp/phn-scratch && cp -r fixtures/synthetic/data /tmp/phn-scratch
# ...run the tools through Claude Desktop (or the MCP Inspector) with PHN_DATA_DIR=/tmp/phn-scratch...
python3 -I a11y_sweep.py target/release/phn-mcp-server /tmp/phn-scratch
```

The script was tested on a DOCX and PDF made by `export_document` (both `ok`, 7 and 5 checks
passed, 0 failed) and on the nurses' user guide `docs/user-guide/PHN-MCP-Server-User-Guide.docx`
(8 passed, 0 failed, 0 warnings, 3 manual), using an older debug build.

- [ ] The sweep reports zero failures and zero warnings, or each remaining finding is recorded with a decision.
- [ ] **Manual checks** (the automated tool cannot do these). Pick one DOCX from each of the tool families that make letters, plans, and reports, and one PDF:
  - [ ] Open the DOCX in Word and run *Review, Check Accessibility*; no errors.
  - [ ] Open the PDF in Acrobat Pro (*Accessibility, Full Check*) or PAC; record the result. PDFs from `export_document` are single-column US Letter without a drawn structure for every element: DOCX is the recommended format for accessible distribution (`STATUS.md`).
  - [ ] Read one document with a screen reader (NVDA, VoiceOver); headings, table headers, and alt text read sensibly.
  - [ ] Color is not the only carrier of meaning in charts and maps (the chart palette is the validated one in `crates/phn-docs/src/palette.rs`; ADR 0006).
  - [ ] Scripts outside the bundled DejaVu font show as `?` in PDF, SVG, and PNG (known limit); confirm the tool tells the nurse to use DOCX for those languages.
  - [ ] Document language is set (the `language` argument of `export_document`), and Spanish and other-language documents carry the right `lang`.

## 8. Threat-model items to close or accept before v1.0

Copy each finding from `docs/security/threat-model.md` section 7 into the release record with a
status: fixed (commit), accepted (who, why), or deferred (issue). Items that are best fixed first
are marked **first**.

- [ ] F1 model-changeable suppression threshold and persistent letterhead (pin in client config at least) **first**
- [ ] F2 input size limits **first**
- [ ] F3 CSV formula injection in `save_watchlist`
- [ ] F4 panic text returned to the model
- [ ] F5 `new_output_file` create-new
- [ ] F6 `PHN_LOG` debug logs tool arguments
- [ ] F7 unscreened file names
- [ ] F8 pin GitHub Actions by SHA, Dependabot, split scan and publish jobs
- [ ] F9 `.trivyignore` expiry 2026-12-11; first content `review_due` 2027-02-01
- [ ] F10 `/cache` overrides are self-attesting (document)
- [ ] F11 leak test asserts success per row
- [ ] F12 keys in query strings (document)
- [ ] F13 third-party license notices shipped with the image
- [ ] F14 validator checks `reviewed_by`; runtime flags overdue content

## 9. Release record

| Field | Value |
|---|---|
| Date | |
| Commit (`git rev-parse HEAD`) | |
| Version and tag | |
| Run by | |
| `cargo deny check` | |
| `cargo audit` | |
| Leak test and related tests | |
| Image digest | |
| Image size, uncompressed and compressed, amd64 | |
| Image size, uncompressed and compressed, arm64 | |
| Trivy result | |
| `check` output saved at | |
| Startup: `initialize` / `tools/list` (binary, median and max) | |
| Startup: `initialize` / `tools/list` (container, cold and warm) | |
| `tools/list` bytes: all / school_nurse / cd_nurse / phn_leader / preparedness | |
| Peak memory | |
| Accessibility sweep result | |
| Manual accessibility checks | |
| Threat-model items closed / accepted / deferred | |
| Content review state shipped (pending or reviewed) and the first `review_due` | |
| Signed by (product owner) | |
