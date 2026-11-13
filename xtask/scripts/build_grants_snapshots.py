#!/usr/bin/env python3
"""Build the Phase 17 grants snapshot (PLAN-06 search_grant_opportunities).

Usage (from the repository root, Python 3.9+, standard library only, and the
`pdftotext` program from poppler-utils on the PATH):

    python3 -I xtask/scripts/build_grants_snapshots.py WORK_DIR [--check]

WORK_DIR is an empty scratch folder for the download (never the repository).
The script downloads the Operational Services Division's weekly "New Bids
Available" list from mass.gov, reads its table with `pdftotext -layout`, writes
snapshots/commbuys_weekly_bids.json, and prints the manifest fields (source
checksum, row count, file checksum) to paste into snapshots/manifest.json.

With --check it writes nothing and reports whether the file would change. The
list is replaced every week, so "changed" is expected; a parse failure (no rows,
or rows without a bid number) exits with status 2.

Snapshot:

* commbuys_weekly_bids.json  Every bid on the weekly list: organization id,
  organization, closing date and time, description, and COMMBUYS bid number.
  The list does not give a bid's type, amount, eligibility, or a link; the tool
  tags topics by keyword and tells the nurse to search COMMBUYS for the bid
  number. The weekly list is mostly construction and purchasing bids; it is
  kept whole so that the tool's topic filter, not this script, decides what is
  relevant to a health department.

Source: https://www.mass.gov/doc/commbuys-home-page-bid-count/download
(the page "Learn about COMMBUYS Resources" and "Recent Procurement Activity" on
mass.gov point to COMMBUYS, https://www.commbuys.com/bso/, which is not
reachable from the build environment).
"""

import hashlib
import json
import re
import subprocess
import sys
import urllib.request
from datetime import datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
UA = "phn-mcp-server snapshot builder (+https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server)"
URL = "https://www.mass.gov/doc/commbuys-home-page-bid-count/download"
PAGE = "https://www.mass.gov/learn-about-commbuys-resources"
OUT = "commbuys_weekly_bids.json"

MONTHS = {m: i for i, m in enumerate(
    ["january", "february", "march", "april", "may", "june", "july", "august",
     "september", "october", "november", "december"], start=1)}
BID_NO = re.compile(r"BD-\d{2}-\d+-[A-Za-z0-9]+-[A-Za-z0-9]+-\d+")


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def fetch(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def clean(s: str) -> str:
    return re.sub(r"\s+", " ", s).strip()


def parse_date(text: str):
    m = re.match(r"([A-Za-z]+)\s+(\d{1,2}),?\s+(\d{4})$", clean(text))
    if not m or m.group(1).lower() not in MONTHS:
        return None
    d = datetime(int(m.group(3)), MONTHS[m.group(1).lower()], int(m.group(2)))
    return d.strftime("%Y-%m-%d")


ROW = re.compile(
    r"^(?P<org_id>\d{3,5})\s+(?P<org>.*?)\s+(?P<date>(?:" + "|".join(m.capitalize() for m in MONTHS) + r")\s+\d{1,2},?(?:\s+\d{4})?)\s+"
    r"(?P<time>\d{1,2}:\d{2}\s*[AP]M)\s+(?P<desc>.*?)\s+(?P<bid>BD-\S+)\s*$")


def parse_layout(text: str):
    """Rows of the layout text. A row's first line holds the date, time, and bid
    number; later lines (wrapped organization, year, description) are cut at the
    column positions of that first line."""
    list_date = None
    m = re.search(r"^\s*(?:Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday),\s+([A-Za-z]+ \d{1,2}, \d{4})\s*$",
                  text, re.M)
    if m:
        list_date = parse_date(m.group(1))
    rows = []
    cur = None
    for line in text.replace("\f", "\n").splitlines():
        if not line.strip() or line.startswith("Org ID"):
            continue
        r = ROW.match(line)
        if r:
            cur = {"pos": (r.start("org"), r.start("date"), r.start("time"), r.start("desc"), r.start("bid")),
                   "org_id": r["org_id"], "organization": [r["org"]], "date": [r["date"]],
                   "time": r["time"], "description": [r["desc"]], "bid_number": r["bid"]}
            rows.append(cur)
        elif cur is not None and re.match(r"\s", line):
            o, d, t, de, b = cur["pos"]
            # The year sits at the end of the date cell; take it out first, because a
            # row split by a page break can be cut through it.
            y = re.search(r"(?<=\s)(\d{4})(?=\s|$)", line[: max(b, 0)])
            if y and not re.search(r"\d{4}", " ".join(cur["date"])) and y.start() < de:
                cur["date"].append(y.group(1))
                line = line[: y.start()] + " " * 4 + line[y.end():]
            for key, cell in (("organization", line[o:d]), ("date", line[d:t]), ("description", line[de:b])):
                if clean(cell):
                    cur[key].append(clean(cell))
    out = []
    for r in rows:
        org = clean(" ".join(r["organization"]))
        date = " ".join(r["date"])
        # A row split by a page break can have its year in the organization cell.
        tail = re.search(r"\s(\d{4})$", org)
        if tail and not re.search(r"\d{4}", date):
            date += " " + tail.group(1)
            org = org[: tail.start()]
        out.append({
            "org_id": r["org_id"],
            "organization": org,
            "bid_date": parse_date(date),
            "bid_time": clean(r["time"]),
            "description": clean(" ".join(r["description"])),
            "bid_number": r["bid_number"],
        })
    return list_date, out


def build(work: Path):
    pdf = fetch(URL)
    (work / "weekly_bids.pdf").write_bytes(pdf)
    txt = subprocess.run(["pdftotext", "-layout", str(work / "weekly_bids.pdf"), "-"],
                         check=True, capture_output=True, text=True).stdout
    list_date, rows = parse_layout(txt)
    bad = [r for r in rows if not BID_NO.fullmatch(r["bid_number"]) or not r["bid_date"] or not r["description"]]
    if not rows or bad or not list_date:
        print(f"parse problem: {len(rows)} rows, {len(bad)} incomplete, list date {list_date}", file=sys.stderr)
        for r in bad[:5]:
            print(r, file=sys.stderr)
        sys.exit(2)
    doc = {
        "list_title": "New Bids Available (COMMBUYS weekly list)",
        "list_date": list_date,
        "source_url": URL,
        "bids": rows,
    }
    return doc, sha(pdf)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    check = "--check" in sys.argv
    if len(args) != 1:
        sys.exit(__doc__)
    work = Path(args[0])
    work.mkdir(parents=True, exist_ok=True)
    if work.resolve().is_relative_to(ROOT.resolve()):
        sys.exit("WORK_DIR must be outside the repository")
    doc, src_sha = build(work)
    new = (json.dumps(doc, indent=1, ensure_ascii=False) + "\n").encode("utf-8")
    path = SNAP / OUT
    old = path.read_bytes() if path.exists() else b""
    changed = old != new
    if changed and not check:
        path.write_bytes(new)
    print(f"{OUT}: sha256 {sha(new)}, bytes {len(new)}, rows {len(doc['bids'])}, list date {doc['list_date']}")
    print("source", URL, "sha256:", src_sha, "(page:", PAGE + ")")
    if check:
        print("changed (the list is replaced weekly)" if changed else "unchanged")
        sys.exit(0)


if __name__ == "__main__":
    main()
