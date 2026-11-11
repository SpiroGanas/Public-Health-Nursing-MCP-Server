#!/usr/bin/env python3
"""Build the Phase 14 vectors-and-housing snapshots (ENV-06 vector_borne_disease_risk).

Usage (from the repository root, Python 3.9+, standard library only):

    python3 -I xtask/scripts/build_vectorhousing_snapshots.py WORK_DIR [--check]

WORK_DIR is an empty scratch folder for downloads (never the repository). The
script downloads the DPH sources, writes the snapshot files to snapshots/, and
prints the manifest fields (source checksums, row counts, file checksums) to
paste into snapshots/manifest.json. With --check it writes nothing and reports
whether the files would change.

Snapshots (all from mass.gov, Massachusetts Department of Public Health):

* ma_arbovirus_risk.csv       EEE and WNV risk level by city/town for every
  season DPH lists on its "Massachusetts arbovirus update" page (2022 on), from
  each season's "EEE and WNV risk level and data" workbook ("Risk by Town"
  sheet). One row per season and town, with the date DPH last updated it.
* ma_arbovirus_positives.csv  The workbooks' "Positive Samples and Cases"
  sheet, counted by season, virus, sample type, county, and town. Human cases
  are counted by county only (DPH does not publish their town); their age,
  sex, and clinical presentation are not kept.
* ma_tickborne_cases.csv      DPH Tick-borne Disease Surveillance Summaries
  2022-2025: Lyme disease (probable), babesiosis and human granulocytic
  anaplasmosis (confirmed and probable) cases and DPH's incidence rates per
  100,000 by county of residence, plus the statewide ten-year totals table of
  the latest summary (Lyme there counts confirmed, probable, and suspect cases).
* ma_tickborne_ed_visits.csv  DPH Tick Exposure and Tick-borne Disease
  Syndromic Surveillance Reports: emergency department visits with a
  tick-borne disease diagnosis by county of residence (Table 1), for each
  January-December report from 2021 and the latest report of the current year.

Town names: DPH writes "Manchester" (MassGIS: Manchester-by-the-Sea), once
"Mattapoissett", and sometimes a Boston neighborhood; these are mapped to the
MassGIS name. "Devens" (not a municipality) is kept as reported.
"""

import csv
import datetime
import hashlib
import io
import re
import sys
import urllib.request
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
UA = "phn-mcp-server snapshot builder (+https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server)"

ARBO_PAGE = "https://www.mass.gov/info-details/massachusetts-arbovirus-update"
TICK_REPORTS_PAGE = "https://www.mass.gov/lists/monthly-tick-borne-disease-reports"
TBD_SUMMARY = "https://www.mass.gov/doc/tickborne-disease-surveillance-report-{year}/download"
TBD_YEARS = [2022, 2023, 2024, 2025]
ED_FIRST_YEAR = 2021

TOWN_ALIASES = {
    "manchester": "Manchester-by-the-Sea",
    "mattapoissett": "Mattapoisett",
    "allston": "Boston",
    "brighton": "Boston",
    "hyde park": "Boston",
    "mattapan": "Boston",
    "roxbury": "Boston",
    "dorchester": "Boston",
    "jamaica plain": "Boston",
    "west roxbury": "Boston",
    "roslindale": "Boston",
    "charlestown": "Boston",
    "east boston": "Boston",
    "south boston": "Boston",
}
LEVELS = {"remote", "low", "moderate", "high", "critical"}
COUNTY_NAMES = {
    "BARNSTABLE": "Barnstable", "BERKSHIRE": "Berkshire", "BRISTOL": "Bristol",
    "DUKES/NANTUCKET": "Dukes & Nantucket", "DUKES & NANTUCKET": "Dukes & Nantucket",
    "ESSEX": "Essex", "FRANKLIN": "Franklin", "HAMPDEN": "Hampden", "HAMPSHIRE": "Hampshire",
    "MIDDLESEX": "Middlesex", "NORFOLK": "Norfolk", "PLYMOUTH": "Plymouth", "SUFFOLK": "Suffolk",
    "WORCESTER": "Worcester", "STATE TOTAL": "Massachusetts",
}

SS = "{http://schemas.openxmlformats.org/spreadsheetml/2006/main}"
RELS = "{http://schemas.openxmlformats.org/officeDocument/2006/relationships}"
W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"


def get(url, dest):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=300) as r:
        data = r.read()
    dest.write_bytes(data)
    return data


def sha(b):
    return hashlib.sha256(b).hexdigest()


def num(s):
    s = str(s).strip().replace(",", "")
    return int(s) if s else None


def excel_date(v):
    v = str(v).strip()
    if not v:
        return ""
    if re.fullmatch(r"\d+(\.\d+)?", v):
        return (datetime.date(1899, 12, 30) + datetime.timedelta(days=int(float(v)))).isoformat()
    return v[:10]


def canonical_town(name):
    n = " ".join(name.strip().split())
    return TOWN_ALIASES.get(n.lower(), n)


# ---------------------------------------------------------------- XLSX / DOCX readers


def col_index(ref):
    n = 0
    for c in re.match(r"[A-Z]+", ref).group(0):
        n = n * 26 + ord(c) - 64
    return n - 1


def read_xlsx(data):
    """{sheet name: [[cell text]]} from workbook bytes."""
    z = zipfile.ZipFile(io.BytesIO(data))
    shared = []
    if "xl/sharedStrings.xml" in z.namelist():
        for si in ET.fromstring(z.read("xl/sharedStrings.xml")).findall(SS + "si"):
            shared.append("".join(t.text or "" for t in si.iter(SS + "t")))
    wb = ET.fromstring(z.read("xl/workbook.xml"))
    rels = {r.get("Id"): r.get("Target") for r in ET.fromstring(z.read("xl/_rels/workbook.xml.rels"))}
    out = {}
    for s in wb.find(SS + "sheets"):
        target = rels[s.get(RELS + "id")].lstrip("/")
        if not target.startswith("xl/"):
            target = "xl/" + target
        rows = []
        for row in ET.fromstring(z.read(target)).iter(SS + "row"):
            cells = {}
            for c in row.findall(SS + "c"):
                t, v = c.get("t"), c.find(SS + "v")
                if t == "s" and v is not None:
                    val = shared[int(v.text)]
                elif t == "inlineStr":
                    val = "".join(x.text or "" for x in c.iter(SS + "t"))
                else:
                    val = v.text if v is not None else ""
                cells[col_index(c.get("r"))] = val
            if cells:
                rows.append([cells.get(i, "") for i in range(max(cells) + 1)])
        out[s.get("name").strip()] = rows
    return out


def para_text(p):
    out = []
    for node in p.iter():
        if node.tag == W + "t":
            out.append(node.text or "")
        elif node.tag == W + "tab":
            out.append("\t")
    return "".join(out).strip()


def docx_blocks(data):
    """Body blocks in order: ("p", text) or ("table", [[cell text]])."""
    z = zipfile.ZipFile(io.BytesIO(data))
    body = ET.fromstring(z.read("word/document.xml")).find(W + "body")
    out = []
    for el in body:
        if el.tag == W + "p":
            t = para_text(el)
            if t:
                out.append(("p", t))
        elif el.tag == W + "tbl":
            rows = []
            for tr in el.iter(W + "tr"):
                rows.append([" ".join(para_text(p) for p in tc.iter(W + "p")).strip()
                             for tc in tr.findall(W + "tc")])
            out.append(("table", rows))
    return out


# ---------------------------------------------------------------- arbovirus


def arbovirus(work):
    page = get(ARBO_PAGE, work / "arbovirus_update.html").decode("utf-8", "replace")
    links = {}
    for m in re.finditer(r'href="((?:https://www\.mass\.gov)?/doc/(\d{4})-eee-and-wnv-risk-level-and-data[a-z0-9-]*/download)"', page):
        url = m.group(1)
        if url.startswith("/"):
            url = "https://www.mass.gov" + url
        links.setdefault(int(m.group(2)), url)
    if not links:
        sys.exit("no EEE and WNV risk workbooks found on the arbovirus update page")
    risk = [["season", "town", "county", "eee_level", "wnv_level", "as_of"]]
    pos_counts = {}
    sources = {}
    urls = []
    for season in sorted(links):
        url = links[season]
        urls.append(url)
        data = get(url, work / f"arbovirus_{season}.xlsx")
        sources[f"{season}-eee-and-wnv-risk-level-and-data.xlsx"] = sha(data)
        sheets = read_xlsx(data)
        rbt = sheets["Risk by Town"]
        head = [h.strip().lower() for h in rbt[0]]
        it, ic = head.index("town"), head.index("county")
        ie, iw = head.index("eee risk level"), head.index("wnv risk level")
        idate = next(i for i, h in enumerate(head) if h in ("last updated", "data as of"))
        seen = set()
        for r in rbt[1:]:
            r = r + [""] * (len(head) - len(r))
            town = r[it].strip()
            if not town:
                continue
            eee, wnv = r[ie].strip().title(), r[iw].strip().title()
            for lv in (eee, wnv):
                if lv.lower() not in LEVELS:
                    sys.exit(f"{season}: unknown risk level {lv!r} for {town}")
            name = canonical_town(town)
            if name in seen:
                sys.exit(f"{season}: town {name} listed twice")
            seen.add(name)
            risk.append([season, name, r[ic].strip(), eee, wnv, excel_date(r[idate])])
        if len(seen) != 351:
            sys.exit(f"{season}: {len(seen)} towns, expected 351")
        ps = sheets["Positive Samples and Cases"]
        head = [h.strip().lower() for h in ps[0]]
        iv, ity, ico = head.index("virus"), head.index("sample type"), head.index("county")
        icm, idt = head.index("community"), head.index("date")
        for r in ps[1:]:
            r = r + [""] * (len(head) - len(r))
            virus = r[iv].strip().upper()
            if not virus:
                continue
            kind = r[ity].strip().title()
            if kind not in ("Mosquito", "Animal", "Human"):
                sys.exit(f"{season}: unknown sample type {kind!r}")
            town = "" if kind == "Human" else canonical_town(r[icm])
            key = (season, virus, kind, r[ico].strip(), town)
            n, latest = pos_counts.get(key, (0, ""))
            d = "" if kind == "Human" else excel_date(r[idt])
            pos_counts[key] = (n + 1, max(latest, d))
    positives = [["season", "virus", "sample_type", "county", "town", "count", "latest_date"]]
    for key in sorted(pos_counts):
        n, latest = pos_counts[key]
        positives.append(list(key) + [n, latest])
    latest_season = max(links)
    as_of = max(r[5] for r in risk[1:] if r[0] == latest_season)
    return risk, positives, sources, urls, latest_season, as_of


# ---------------------------------------------------------------- tick-borne disease


def county_table(rows, what):
    out = []
    for r in rows[1:]:
        if len(r) < 3 or not r[0].strip():
            continue
        name = COUNTY_NAMES.get(r[0].strip().upper())
        if name is None:
            sys.exit(f"{what}: unknown county {r[0]!r}")
        out.append((name, num(r[1]), r[2].strip().replace(",", "")))
    if len(out) != 14:
        sys.exit(f"{what}: {len(out)} rows, expected 13 counties and the state total")
    return out


def tick_cases(work):
    rows = [["year", "disease", "area", "cases", "rate_per_100k", "case_classes", "report"]]
    sources = {}
    ten_year = None
    for year in TBD_YEARS:
        data = get(TBD_SUMMARY.format(year=year), work / f"tickborne_summary_{year}.docx")
        sources[f"tickborne-disease-surveillance-report-{year}.docx"] = sha(data)
        label = ""
        found = set()
        for kind, val in docx_blocks(data):
            if kind == "p":
                label = val
                continue
            head = " ".join(val[0]).lower() if val else ""
            if label.startswith("Table 1") and "lyme" in label.lower():
                disease, classes = "Lyme disease", "probable"
            elif label.startswith("Table 2") and "babesiosis" in label.lower():
                disease, classes = "Babesiosis", "confirmed and probable"
            elif label.startswith("Table 3") and "hga" in label.lower():
                disease, classes = "Anaplasmosis (HGA)", "confirmed and probable"
            elif head.startswith("year") and "babesiosis" in head and year == TBD_YEARS[-1]:
                ten_year = val
                continue
            else:
                continue
            for area, cases, rate in county_table(val, f"{year} {disease}"):
                rows.append([year, disease, area, cases, rate, classes, f"Tick-borne Disease Surveillance Summary, {year}"])
            found.add(disease)
        if len(found) != 3:
            sys.exit(f"{year} summary: found tables for {sorted(found)}")
    if ten_year is None:
        sys.exit("ten-year totals table not found in the latest summary")
    head = [h.strip().lower() for h in ten_year[0]]
    names = {"babesiosis": ("Babesiosis", "confirmed and probable"),
             "hga": ("Anaplasmosis (HGA)", "confirmed and probable"),
             "lyme": ("Lyme disease", "confirmed, probable, and suspect")}
    latest = TBD_YEARS[-1]
    for r in ten_year[1:]:
        if not r[0].strip().isdigit():
            continue
        for i, h in enumerate(head[1:], start=1):
            disease, classes = names[h]
            rows.append([int(r[0]), disease, "Massachusetts (ten-year table)", num(r[i]), "", classes,
                         f"Tick-borne Disease Surveillance Summary, {latest} (Figure 1 table)"])
    return rows, sources


def ed_visits(work):
    page = get(TICK_REPORTS_PAGE, work / "tick_reports.html").decode("utf-8", "replace")
    links = {}
    for m in re.finditer(r'href="((?:https://www\.mass\.gov)?/doc/tick-exposure-and-tick-?borne-disease-syndromic-surveillance-report-([a-z0-9-]+)/download)"', page):
        url = m.group(1)
        if url.startswith("/"):
            url = "https://www.mass.gov" + url
        slug = m.group(2)
        ym = re.search(r"(january|february|march|april|may|june|july|august|september|october|november|december)-(\d{4})$", slug)
        if not ym:
            continue
        month = ["january", "february", "march", "april", "may", "june", "july", "august",
                 "september", "october", "november", "december"].index(ym.group(1)) + 1
        year = int(ym.group(2))
        if year < ED_FIRST_YEAR:
            continue
        prev = links.get(year)
        if prev is None or month > prev[0]:
            links[year] = (month, url)
    rows = [["year", "through_month", "county", "ed_visits", "tickborne_disease_visits", "per_10k_ed_visits"]]
    sources = {}
    for year in sorted(links):
        month, url = links[year]
        data = get(url, work / f"tick_ed_{year}.docx")
        sources[f"tick-syndromic-report-{year}-{month:02d}.docx"] = sha(data)
        table = next((v for k, v in docx_blocks(data) if k == "table" and v and v[0] and v[0][0].strip().lower() == "county"), None)
        if table is None:
            sys.exit(f"{year}: county table not found")
        n = 0
        for r in table[1:]:
            if not r or not r[0].strip():
                continue
            name = COUNTY_NAMES.get(r[0].strip().upper())
            if name is None:
                sys.exit(f"{year} ED report: unknown county {r[0]!r}")
            rows.append([year, f"{year}-{month:02d}", name, num(r[1]), num(r[2]), r[3].strip()])
            n += 1
        if n != 13:
            sys.exit(f"{year} ED report: {n} county rows")
    return rows, sources, sorted(links), links


# ---------------------------------------------------------------- output


def to_csv(rows):
    out = io.StringIO()
    w = csv.writer(out, lineterminator="\n")
    w.writerows(rows)
    return out.getvalue()


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    check = "--check" in sys.argv
    if len(args) != 1:
        sys.exit(__doc__)
    work = Path(args[0])
    work.mkdir(parents=True, exist_ok=True)
    if work.resolve().is_relative_to(ROOT.resolve()):
        sys.exit("WORK_DIR must be outside the repository")

    risk, positives, arbo_src, arbo_urls, season, as_of = arbovirus(work)
    cases, tbd_src = tick_cases(work)
    ed, ed_src, ed_years, ed_links = ed_visits(work)
    files = {
        "ma_arbovirus_risk.csv": to_csv(risk),
        "ma_arbovirus_positives.csv": to_csv(positives),
        "ma_tickborne_cases.csv": to_csv(cases),
        "ma_tickborne_ed_visits.csv": to_csv(ed),
    }
    changed = []
    for name, text in files.items():
        path = SNAP / name
        old = path.read_bytes() if path.exists() else b""
        new = text.encode("utf-8")
        if old != new:
            changed.append(name)
            if not check:
                path.write_bytes(new)
        print(f'{name}: sha256 {sha(new)}, bytes {len(new)}, rows {text.count(chr(10)) - 1}')
    print(f"arbovirus seasons {sorted({r[0] for r in risk[1:]})}; latest {season}, as of {as_of}")
    print("arbovirus workbooks:", *arbo_urls, sep="\n  ")
    print("arbovirus source sha256:", arbo_src)
    print("tick-borne summaries sha256:", tbd_src)
    last_ed = ed_years[-1]
    print(f"ED reports {ed_years}; latest {last_ed} through month {ed_links[last_ed][0]}")
    print("ED reports sha256:", ed_src)
    if check:
        print("changed:" if changed else "unchanged", *changed)
        sys.exit(1 if changed else 0)


if __name__ == "__main__":
    main()
