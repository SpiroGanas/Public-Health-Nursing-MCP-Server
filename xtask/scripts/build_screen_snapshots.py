#!/usr/bin/env python3
"""Build the Phase 15 screening snapshot (CHR-03): Massachusetts diabetes prevention programs.

Usage (from the repository root, Python 3.9+, standard library only):

    python3 -I xtask/scripts/build_screen_snapshots.py WORK_DIR [--check]

WORK_DIR is an empty scratch folder for the downloaded page (never the repository).
The script downloads the Massachusetts Department of Public Health (DPH) page
"Diabetes Prevention Programs in Massachusetts", writes snapshots/ma_dpp_programs.csv,
and prints the manifest fields (checksum, bytes, rows, source checksum) to paste into
snapshots/manifest.json. With --check it writes nothing and reports whether the file
would change (exit status 1 when it would).

Why this source, and not CDC's registry: the National Diabetes Prevention Program
registry (CDC Diabetes Prevention Recognition Program, dprp.cdc.gov and
dprpdataportal.cdc.gov) was not reachable from the build environment on 2026-11-12
(the proxy refused both hosts; the older nccd.cdc.gov/DDT_DPRP pages return 404, so the
ASP.NET form posts that worked for other nccd.cdc.gov applications have nothing to post
to). DPH's page is the state's own list of "Active MA DPPs" with the town, site, address,
and a contact. It does not say which sites are CDC-recognized, the delivery mode, or when
it was last updated. The file therefore carries the page as published and the tool says
so: recognition status and delivery mode are "not stated".

Privacy: DPH's contact column mixes program coordinators' names and personal work emails
with organization phone numbers. Only organization-level contacts are kept: a phone number
or email address is kept only when the contact cell has no person's name. Names are never
kept (the CSV has no name column).

Columns of ma_dpp_programs.csv:
    municipality   Massachusetts city or town resolved from the address ("" when the site
                   is outside Massachusetts)
    county         county of that municipality
    listed_place   the "City/Town" column as DPH printed it (may be a village or
                   neighborhood, e.g. "Boston - Dorchester")
    site_name      the "Location" column
    street         street line of the address
    city           city printed in the address
    state          two-letter state of the address
    zip            ZIP code
    org_phone      phone number, only when the contact cell names no person
    org_email      email address, only when the contact cell names no person
    has_named_contact  yes when DPH lists a named contact person for the site (not kept)
"""

import csv
import hashlib
import html
import io
import json
import re
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
UA = "phn-mcp-server snapshot builder (+https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server)"
PAGE = "https://www.mass.gov/service-details/diabetes-prevention-programs-in-massachusetts"
OUT = "ma_dpp_programs.csv"

FIELDS = ["municipality", "county", "listed_place", "site_name", "street", "city", "state", "zip",
          "org_phone", "org_email", "has_named_contact"]

# Villages and neighborhoods DPH prints as the city, with the municipality that contains them.
VILLAGES = {
    "dorchester": "Boston", "roxbury": "Boston", "hyde park": "Boston", "east boston": "Boston",
    "hyannis": "Barnstable", "west barnstable": "Barnstable", "newtonville": "Newton",
    "foxboro": "Foxborough", "middleboro": "Middleborough", "north attleboro": "North Attleborough",
}


def sha(b):
    return hashlib.sha256(b).hexdigest()


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def norm(s):
    return re.sub(r"[^a-z0-9]+", " ", s.lower()).strip()


def towns():
    out = {}
    with open(SNAP / "ma_municipalities.csv", encoding="utf-8") as f:
        for r in csv.DictReader(f):
            out[norm(r["name"])] = (r["name"], r["county"])
    return out


def cell_lines(fragment):
    """Text lines of a table cell (one per <br>), with tags removed."""
    s = re.sub(r"<br\s*/?>", "\n", fragment, flags=re.I)
    s = re.sub(r"<[^>]+>", "", s)
    s = html.unescape(s).replace("\xa0", " ")
    return [re.sub(r"\s+", " ", ln).strip() for ln in s.split("\n") if ln.strip()]


EMAIL = re.compile(r"[\w.+-]+@[\w-]+(?:\.[\w-]+)+")
PHONE = re.compile(r"\(?\d{3}\)?[ -]?\d{3}-\d{4}(?:,? ?(?:Ext|ext)\.? ?\d+)?")


def parse_contact(lines):
    """(phone, email, has_named_contact). A line that is neither a phone number nor an email
    address is a person's name; when there is one, nothing is kept."""
    named = False
    phones, emails = [], []
    for ln in lines:
        e = EMAIL.findall(ln)
        p = PHONE.findall(ln)
        rest = PHONE.sub("", EMAIL.sub("", ln)).strip(" ,;()-")
        if e:
            emails.extend(e)
        if p:
            phones.extend(p)
        if rest and not e and not p:
            named = True
    if named:
        return "", "", True
    return (phones[0] if phones else ""), (emails[0] if emails else ""), False


def parse_address(lines):
    """(street, city, state, zip). The last line is "City, ST ZIP" (commas vary); the lines
    before it are the street."""
    if len(lines) < 2:
        return " ".join(lines), "", "", ""
    m = re.match(r"^(?P<city>[A-Za-z .'-]+?),?\s+(?P<state>[A-Z]{2}),?\s+(?P<zip>\d{5})$", lines[-1])
    if not m:
        return " ".join(lines), "", "", ""
    return " ".join(lines[:-1]).strip(" ,"), m["city"].strip(), m["state"], m["zip"]


def parse(page):
    t = page.decode("utf-8")
    i = t.find("Active MA DPPs")
    if i < 0:
        raise SystemExit("the DPH page no longer has an 'Active MA DPPs' heading; check the page")
    table = re.search(r"<table.*?</table>", t[i:], flags=re.S)
    if not table:
        raise SystemExit("no table after 'Active MA DPPs'")
    rows = re.findall(r"<tr>(.*?)</tr>", table.group(0), flags=re.S)
    head = [cell_lines(c) for c in re.findall(r"<th[^>]*>(.*?)</th>", rows[0], flags=re.S)]
    if [h[0] for h in head] != ["City/Town", "Location", "Address", "Contact"]:
        raise SystemExit(f"unexpected table columns: {head}")
    tw = towns()
    out = []
    for r in rows[1:]:
        cells = re.findall(r"<td[^>]*>(.*?)</td>", r, flags=re.S)
        if len(cells) != 4:
            raise SystemExit(f"row with {len(cells)} cells: {r[:200]}")
        place = " ".join(cell_lines(cells[0]))
        site = " ".join(cell_lines(cells[1]))
        street, city, state, zipc = parse_address(cell_lines(cells[2]))
        phone, email, named = parse_contact(cell_lines(cells[3]))
        muni, county = "", ""
        if state == "MA":
            key = VILLAGES.get(norm(city), city)
            if norm(key) not in tw:
                raise SystemExit(f"cannot place '{city}' ({place}, {site}) in a Massachusetts municipality")
            muni, county = tw[norm(key)]
        out.append([muni, county, place, site, street, city, state, zipc, phone, email,
                    "yes" if named else "no"])
    return out


def to_csv(rows):
    buf = io.StringIO()
    w = csv.writer(buf, lineterminator="\n")
    w.writerow(FIELDS)
    w.writerows(rows)
    return buf.getvalue()


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if not args:
        raise SystemExit(__doc__)
    work = Path(args[0]).resolve()
    if ROOT in work.parents or work == ROOT:
        raise SystemExit("WORK_DIR must be outside the repository")
    work.mkdir(parents=True, exist_ok=True)
    check = "--check" in sys.argv
    cached = work / "dph_dpp_page.html"
    if cached.exists() and cached.stat().st_size > 0:
        page = cached.read_bytes()
    else:
        page = fetch(PAGE)
        cached.write_bytes(page)
    rows = parse(page)
    if len(rows) < 50:
        raise SystemExit(f"only {len(rows)} programs parsed; the page layout probably changed")
    data = to_csv(rows).encode("utf-8")
    path = SNAP / OUT
    changed = not path.exists() or path.read_bytes() != data
    if not check:
        path.write_bytes(data)
    print(json.dumps({"path": OUT, "sha256": sha(data), "bytes": len(data), "rows": len(rows),
                      "source_sha256": {"dph_dpp_page.html": sha(page)},
                      "changed": changed}, indent=1))
    if check and changed:
        sys.exit(1)


if __name__ == "__main__":
    main()
