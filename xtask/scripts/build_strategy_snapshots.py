#!/usr/bin/env python3
"""Build the Phase 18 strategy snapshots (LEAD-03 assess_foundational_public_health_services).

Usage (from the repository root, Python 3.9+, standard library only):

    python3 -I xtask/scripts/build_strategy_snapshots.py [--check] [--date YYYY-MM-DD]

What it reads (both are public files on mass.gov; mass.gov answers 403 to browser-like user
agents, so the requests use a plain curl user agent, as the server's own client does):

* "Performance Standards for Local Public Health" (Excel), published by the Department of Public
  Health's Office of Local and Regional Health on 2023-10-11 (file last modified 2024-07-15).
  The Administration, Disease Control And Prevention, Environmental Health, and Tobacco Use
  Prevention sheets list the mandated local responsibilities (statute or regulation citation,
  a one-line summary, and the state program that supports it). The Workforce Standards sheet lists
  required and recommended credentials by position and by inspection type.
  https://www.mass.gov/doc/performance-standards/download
* "Shared Service Arrangements" (Excel), updated April 2026 (file last modified 2026-04-16):
  which shared service arrangement, lead entity, and training hub each municipality is in.
  https://www.mass.gov/doc/shared-service-arrangements/download

It writes snapshots/ma_performance_standards.json and snapshots/ma_shared_service_arrangements.csv
and updates their entries in snapshots/manifest.json. With --check it writes nothing, rebuilds in
memory from the downloaded files, and exits 1 if the files on disk differ.

Nothing is filled from memory: every row is a row of the downloaded workbook. State contact
phone numbers and email addresses are dropped (program names are kept).
"""

import csv
import datetime
import hashlib
import io
import json
import re
import sys
import urllib.request
import zipfile
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
UA = "curl/8.5.0"
PERF_URL = "https://www.mass.gov/doc/performance-standards/download"
SSA_URL = "https://www.mass.gov/doc/shared-service-arrangements/download"
PERF_FILE = "ma_performance_standards.json"
SSA_FILE = "ma_shared_service_arrangements.csv"
PERF_ID = "ma-performance-standards"
SSA_ID = "ma-shared-service-arrangements"

NS = {"m": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
REL = "{http://schemas.openxmlformats.org/officeDocument/2006/relationships}id"
PKG = "{http://schemas.openxmlformats.org/package/2006/relationships}"

MANDATE_SHEETS = [
    ("Administration", "ADM"),
    ("Disease Control And Prevention", "DIS"),
    ("Environmental Health", "ENV"),
    ("Tobacco Use Prevention", "TOB"),
]

NOT_INCLUDED = [
    "Only the four mandate sheets and the Workforce Standards sheet are read. The workbook's 'Frequently Used' sheet (discretionary board of health powers, such as appointing agents), 'Resources', and 'Keywords' sheets are not included.",
    "The workbook lists responsibilities as DPH summarizes them; the statute or regulation citation controls. A footnote in the workbook says OLRH has received comments that the standards marked * are outdated.",
    "The workbook has no status, due dates, or scoring: those are for each department to record. The next version of the Performance Standards is expected to include the Foundational Public Health Services framework (DPH's FPHS page).",
    "State program contact phone numbers and email addresses are not copied; the program names are kept.",
]


def sha(b):
    return hashlib.sha256(b).hexdigest()


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read(), r.headers.get("Last-Modified", "")


def read_workbook(data):
    """Sheet name -> list of rows (list of str), from an .xlsx using only the standard library."""
    z = zipfile.ZipFile(io.BytesIO(data))
    shared = []
    if "xl/sharedStrings.xml" in z.namelist():
        root = ET.fromstring(z.read("xl/sharedStrings.xml"))
        for si in root.findall("m:si", NS):
            shared.append("".join(t.text or "" for t in si.iter("{%s}t" % NS["m"])))
    wb = ET.fromstring(z.read("xl/workbook.xml"))
    rels = ET.fromstring(z.read("xl/_rels/workbook.xml.rels"))
    target = {r.get("Id"): r.get("Target") for r in rels.findall(PKG + "Relationship")}
    out = {}
    for s in wb.find("m:sheets", NS).findall("m:sheet", NS):
        path = target[s.get(REL)]
        path = path.lstrip("/")
        if not path.startswith("xl/"):
            path = "xl/" + path
        root = ET.fromstring(z.read(path))
        rows = []
        for row in root.iter("{%s}row" % NS["m"]):
            cells = {}
            for c in row.findall("m:c", NS):
                ref = c.get("r")
                col = 0
                for ch in re.match(r"[A-Z]+", ref).group(0):
                    col = col * 26 + ord(ch) - 64
                t = c.get("t")
                v = c.find("m:v", NS)
                if t == "s" and v is not None:
                    val = shared[int(v.text)]
                elif t == "inlineStr":
                    val = "".join(x.text or "" for x in c.iter("{%s}t" % NS["m"]))
                elif v is not None and v.text is not None:
                    val = v.text
                else:
                    val = ""
                cells[col] = val
            width = max(cells) if cells else 0
            rows.append([cells.get(i, "") for i in range(1, width + 1)])
        out[s.get("name")] = rows
    return out


def clean(s):
    return re.sub(r"\s+", " ", (s or "").replace(" ", " ")).strip()


def program_name(contact):
    """State program contact without phone numbers and email addresses."""
    parts = [clean(p) for p in re.split(r"\n|/", contact or "")]
    keep = []
    for p in parts:
        p = re.sub(r"\(?\d{3}\)?[ -]\d{3}-\d{4}", "", p).strip()
        if not p or "@" in p:
            continue
        keep.append(p)
    return " / ".join(keep)


def mandates(rows, code):
    out = []
    topic = ""
    contact = ""
    n = 0
    for r in rows[1:]:
        r = r + [""] * (4 - len(r))
        a, b, c, d = (x for x in r[:4])
        if clean(a) and not clean(b) and not clean(c):
            continue  # footnote or blank
        if clean(b).startswith("*") or clean(a).startswith("*") or clean(a).startswith("**"):
            continue
        if not clean(b):
            continue
        if clean(a):
            topic = clean(a)
        if clean(d):
            contact = program_name(d)
        n += 1
        cite = clean(b)
        outdated = cite.endswith("*")
        out.append(
            {
                "id": f"PS-{code}-{n:03d}",
                "area": topic,
                "citation": cite.rstrip("*").strip(),
                "summary": clean(c),
                "state_program": contact,
                "marked_outdated": outdated,
            }
        )
    return out


def position_name(a):
    """Position label without footnote asterisks; all-capital labels in sentence case."""
    a = a.strip("* ").strip()
    if a.isupper():
        a = a.capitalize()
    elif a.endswith("Member") or a.isupper():
        pass
    return a


def bullets(s):
    items = []
    for line in re.split(r"\n", s or ""):
        line = clean(line.lstrip("• -").strip())
        if line:
            items.append(line)
    return items


def workforce(rows):
    positions = []
    inspections = []
    notes = []
    mode = "positions"
    n = m = 0
    for r in rows:
        r = r + [""] * (4 - len(r))
        a, b, c, d = r[:4]
        a_c = clean(a)
        if not a_c:
            continue
        if a_c.startswith("Position"):
            mode = "positions"
            continue
        if a_c.startswith("Inspection Type"):
            mode = "inspections"
            continue
        if a_c.startswith("Blueprint"):
            continue
        if a_c.startswith("•") or a_c.startswith("*"):
            notes.append(clean(a))
            if clean(b):
                notes.append(clean(b))
            continue
        if mode == "positions":
            n += 1
            positions.append(
                {
                    "id": f"WF-POS-{n:02d}",
                    "position": position_name(a_c),
                    "required_at_hire": bullets(b),
                    "required_after_hire": bullets(c),
                    "recommended_after_hire": bullets(d),
                }
            )
        else:
            m += 1
            inspections.append(
                {
                    "id": f"WF-INS-{m:02d}",
                    "inspection_type": a_c,
                    "required": bullets(b),
                    "recommended": bullets(c),
                }
            )
    return positions, inspections, notes


def build(date):
    perf_bytes, perf_mod = fetch(PERF_URL)
    ssa_bytes, ssa_mod = fetch(SSA_URL)
    wb = read_workbook(perf_bytes)
    all_mandates = []
    for sheet, code in MANDATE_SHEETS:
        all_mandates.extend(mandates(wb[sheet], code))
    positions, inspections, wnotes = workforce(wb["Workforce Standards"])
    perf = {
        "format_version": 1,
        "description": "Massachusetts Performance Standards for Local Public Health: the mandated local responsibilities and the workforce standards, as listed in DPH's Excel workbook (Office of Local and Regional Health, published 2023-10-11).",
        "retrieved": date,
        "source_last_modified": perf_mod,
        "not_included": NOT_INCLUDED,
        "mandates": all_mandates,
        "workforce_positions": positions,
        "workforce_inspection_types": inspections,
        "workforce_notes": wnotes,
    }
    perf_text = json.dumps(perf, indent=1, ensure_ascii=False) + "\n"

    ssa_rows = read_workbook(ssa_bytes)["Sheet1"]
    out = io.StringIO()
    w = csv.writer(out, lineterminator="\n")
    w.writerow(["municipality", "arrangement", "lead_entity", "training_hub"])
    rows = []
    for r in ssa_rows[1:]:
        r = r + [""] * (4 - len(r))
        muni = clean(r[0])
        if not muni:
            continue
        arr = clean(r[1])
        lead = clean(r[2])
        hub = clean(r[3])
        if "N/A" in arr:
            arr = ""
        if "N/A" in lead:
            lead = ""
        if hub in ("0", "none"):
            hub = ""
        rows.append((muni, arr, lead, hub))
    rows.sort(key=lambda t: t[0].lower())
    for t in rows:
        w.writerow(t)
    ssa_text = out.getvalue()
    return perf_text, ssa_text, perf_bytes, ssa_bytes, perf_mod, ssa_mod, len(all_mandates), len(rows)


def entries(date, perf_text, ssa_text, perf_bytes, ssa_bytes, perf_mod, ssa_mod, n_perf, n_ssa):
    perf_entry = {
        "id": PERF_ID,
        "title": "Massachusetts Performance Standards for Local Public Health (mandated local responsibilities and workforce standards)",
        "publisher": "Massachusetts Department of Public Health, Office of Local and Regional Health",
        "source_urls": [PERF_URL, "https://www.mass.gov/resource/performance-standards-for-local-public-health"],
        "source_sha256": {"performance-standards.xlsx": sha(perf_bytes)},
        "source_vintage": "published 2023-10-11; workbook last modified " + perf_mod,
        "retrieved": date,
        "license": "Massachusetts state government publication. mass.gov's site policies were not read (the page was not fetched), so check them before republishing the workbook; the snapshot reuses citations and DPH's one-line summaries and drops phone numbers and email addresses.",
        "derivation": "xtask/scripts/build_strategy_snapshots.py reads the Administration, Disease Control And Prevention, Environmental Health, and Tobacco Use Prevention sheets (topic area, citation, summary, state program) and the Workforce Standards sheet, row by row, with the Python standard library. Blank topic-area and contact cells carry down from the row above, as the workbook's merged cells intend. IDs (PS-ADM-001 and so on) are assigned in sheet order by this project and are not DPH identifiers.",
        "caveats": NOT_INCLUDED,
        "files": [
            {
                "path": PERF_FILE,
                "sha256": sha(perf_text.encode()),
                "bytes": len(perf_text.encode()),
                "rows": n_perf,
            }
        ],
    }
    ssa_entry = {
        "id": SSA_ID,
        "title": "Massachusetts shared public health service arrangements, by municipality",
        "publisher": "Massachusetts Department of Public Health, Office of Local and Regional Health (Public Health Excellence Grant Program)",
        "source_urls": [SSA_URL, "https://www.mass.gov/info-details/public-health-excellence-grant-program"],
        "source_sha256": {"shared-service-arrangements.xlsx": sha(ssa_bytes)},
        "source_vintage": "list updated April 2026; workbook last modified " + ssa_mod,
        "retrieved": date,
        "license": "Massachusetts state government publication; mass.gov's site policies were not read.",
        "derivation": "xtask/scripts/build_strategy_snapshots.py reads the workbook's only sheet (cached values of its link formulas), maps the workbook's N/A markers to blank, and sorts by municipality.",
        "caveats": [
            "A blank arrangement means the list shows no shared service arrangement for the municipality (it may be a standalone grantee, such as Boston, Springfield, or Worcester, or not funded). Two rows are labeled 'PHE: Capacity Building Grantee' by DPH.",
            "Membership changes with each grant year. Confirm with the Office of Local and Regional Health before relying on the list.",
        ],
        "files": [
            {
                "path": SSA_FILE,
                "sha256": sha(ssa_text.encode()),
                "bytes": len(ssa_text.encode()),
                "rows": n_ssa,
            }
        ],
    }
    return perf_entry, ssa_entry


def main():
    check = "--check" in sys.argv
    date = datetime.date.today().isoformat()
    if "--date" in sys.argv:
        date = sys.argv[sys.argv.index("--date") + 1]
    manifest_path = SNAP / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    if check:
        for e in manifest["snapshots"]:
            if e["id"] == PERF_ID:
                date = e["retrieved"]
    perf_text, ssa_text, pb, sb, pm, sm, np_, ns = build(date)
    pe, se = entries(date, perf_text, ssa_text, pb, sb, pm, sm, np_, ns)
    if check:
        ok = True
        for name, text in ((PERF_FILE, perf_text), (SSA_FILE, ssa_text)):
            disk = (SNAP / name).read_text()
            if disk != text:
                print(f"{name}: differs from a fresh build", file=sys.stderr)
                ok = False
        sys.exit(0 if ok else 1)
    (SNAP / PERF_FILE).write_text(perf_text)
    (SNAP / SSA_FILE).write_text(ssa_text)
    snaps = [e for e in manifest["snapshots"] if e["id"] not in (PERF_ID, SSA_ID)]
    snaps.extend([pe, se])
    manifest["snapshots"] = snaps
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")
    print(f"wrote {PERF_FILE} ({np_} mandates) and {SSA_FILE} ({ns} municipalities)")


if __name__ == "__main__":
    main()
