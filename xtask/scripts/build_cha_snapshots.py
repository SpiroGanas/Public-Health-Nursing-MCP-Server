#!/usr/bin/env python3
"""Build the Phase 15 community-health-assessment snapshot (EQ-08 community_health_assessment_builder).

Usage (from the repository root, Python 3.9+, standard library only):

    python3 -I xtask/scripts/build_cha_snapshots.py WORK_DIR [--check]

WORK_DIR is an empty scratch folder for downloads (never the repository). The
script downloads the CMS source, writes the snapshot file to snapshots/, and
prints the manifest fields (source checksum, row count, file checksum) to paste
into snapshots/manifest.json. With --check it writes nothing and reports
whether the file would change.

Snapshot:

* ma_hospitals.csv  Massachusetts hospitals from the CMS Provider Data Catalog
  dataset "Hospital General Information" (xubh-q36u): CMS certification number,
  name, street address, town, ZIP code, county, hospital type, ownership, and
  whether it has an emergency department. Only the directory fields are kept
  (no quality ratings). Names are as CMS lists them (title-cased here). The
  tool uses it to list the hospitals in or near a community, whose community
  health needs assessments (CHNAs) the nurse should collect. It does not say
  which hospitals file community benefits reports with the Attorney General
  (non-profit acute care hospitals do; the ownership field is CMS's).

Town names: CMS writes the postal city, which is sometimes a Boston
neighborhood or a village; these are mapped to the municipality name where this
script knows the mapping and are otherwise kept as CMS wrote them (title-cased).
"""

import csv
import hashlib
import io
import json
import sys
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
UA = "phn-mcp-server snapshot builder (+https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server)"
API = "https://data.cms.gov/provider-data/api/1/datastore/query/xubh-q36u/0"
PAGE = "https://data.cms.gov/provider-data/dataset/xubh-q36u"

# Postal city or neighborhood -> municipality (MassGIS name).
TOWN_ALIASES = {
    "allston": "Boston", "brighton": "Boston", "hyde park": "Boston", "mattapan": "Boston",
    "roxbury": "Boston", "dorchester": "Boston", "jamaica plain": "Boston",
    "west roxbury": "Boston", "roslindale": "Boston", "charlestown": "Boston",
    "east boston": "Boston", "south boston": "Boston", "roxbury crossing": "Boston",
    "boston": "Boston", "manchester": "Manchester-by-the-Sea",
    "north dartmouth": "Dartmouth", "south dartmouth": "Dartmouth",
    "hyannis": "Barnstable", "cotuit": "Barnstable", "west barnstable": "Barnstable",
    "south yarmouth": "Yarmouth", "west yarmouth": "Yarmouth",
    "jamaica plain ": "Boston", "north chelmsford": "Chelmsford", "north adams": "North Adams",
    "so weymouth": "Weymouth", "south weymouth": "Weymouth", "north reading": "North Reading",
    "wellesley hills": "Wellesley", "cambridge": "Cambridge", "leeds": "Northampton",
    "south attleboro": "Attleboro", "w concord": "Concord", "west concord": "Concord", "pocasset": "Bourne",
}
SMALL = {"of", "the", "and", "at", "for", "in", "on"}
KEEP_UPPER = {"UMASS", "MGH", "VA", "MA", "II", "III", "CHA", "UMMHC", "MGB", "MCPHS"}


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def fetch(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def title_case(s: str) -> str:
    out = []
    s = s.strip().rstrip("-").strip()  # CMS leaves a trailing " -" on some names
    for i, w in enumerate(s.split()):
        base = w.strip("(),.&/-")
        if base.upper() in KEEP_UPPER or (len(base) <= 3 and base.isupper() and not base.isalpha()):
            out.append(w.upper() if base.upper() in KEEP_UPPER else w)
        elif w.lower() in SMALL and i > 0:
            out.append(w.lower())
        elif w.startswith("MC") and len(w) > 3:
            out.append("Mc" + w[2:].capitalize())
        elif w == "&":
            out.append(w)
        else:
            out.append("-".join(p.capitalize() for p in w.split("-")))
    return " ".join(out)


def town(city: str) -> str:
    key = city.strip().lower()
    if key in TOWN_ALIASES:
        return TOWN_ALIASES[key]
    return title_case(city)


def hospitals(work: Path):
    q = urllib.parse.urlencode(
        {
            "limit": 500,
            "conditions[0][property]": "state",
            "conditions[0][value]": "MA",
            "conditions[0][operator]": "=",
        }
    )
    raw = fetch(f"{API}?{q}")
    (work / "hospital_general_information_ma.json").write_bytes(raw)
    results = json.loads(raw)["results"]
    rows = [["ccn", "name", "address", "town", "zip", "county", "type", "ownership", "emergency_services"]]
    for r in sorted(results, key=lambda x: (x["citytown"], x["facility_name"], x["facility_id"])):
        rows.append(
            [
                r["facility_id"].strip(),
                title_case(r["facility_name"]),
                title_case(r["address"]),
                town(r["citytown"]),
                r["zip_code"].strip()[:5],
                title_case(r["countyparish"]),
                r["hospital_type"].strip(),
                r["hospital_ownership"].strip(),
                r["emergency_services"].strip(),
            ]
        )
    return rows, sha(raw)


def to_csv(rows) -> str:
    buf = io.StringIO()
    csv.writer(buf, lineterminator="\n").writerows(rows)
    return buf.getvalue()


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    check = "--check" in sys.argv
    if len(args) != 1:
        sys.exit(__doc__)
    work = Path(args[0])
    work.mkdir(parents=True, exist_ok=True)
    if work.resolve().is_relative_to(ROOT.resolve()):
        sys.exit("WORK_DIR must be outside the repository")
    rows, src_sha = hospitals(work)
    text = to_csv(rows)
    path = SNAP / "ma_hospitals.csv"
    old = path.read_bytes() if path.exists() else b""
    new = text.encode("utf-8")
    changed = old != new
    if changed and not check:
        path.write_bytes(new)
    print(f"ma_hospitals.csv: sha256 {sha(new)}, bytes {len(new)}, rows {len(rows) - 1}")
    print("source", PAGE, "sha256 of the API response:", src_sha)
    if check:
        print("changed" if changed else "unchanged")
        sys.exit(1 if changed else 0)


if __name__ == "__main__":
    main()
