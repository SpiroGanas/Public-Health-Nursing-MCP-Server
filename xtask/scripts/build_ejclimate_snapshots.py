#!/usr/bin/env python3
"""Build the Phase 14 environmental justice and climate snapshots (ENV-04, ENV-05).

Usage (from the repository root, Python 3.9+, standard library only):

    python3 -I xtask/scripts/build_ejclimate_snapshots.py WORK_DIR [--check]

WORK_DIR is an empty scratch folder for downloads (never the repository). The
script downloads the sources, writes the snapshot files to snapshots/, and
prints the manifest entries (source checksums, row counts, file checksums) to
paste at the end of the `snapshots` array in snapshots/manifest.json. With
--check it writes nothing and reports whether the files would change.

Snapshots (ids in the manifest):

* ma-ej-2020
    ma_ej_2020_block_groups.csv  MassGIS/EEA "2020 Environmental Justice
        Populations" layer (EJ_POLY, updated November 2022, minor fix June
        2024): one row per EJ block group polygon (block groups split at a town
        line appear once per town, with the whole block group's attributes, as
        in the source). Geometry is in the encoding of ma_boundaries_*.csv:
        NAD83 / Massachusetts Mainland (EPSG:26986), Douglas-Peucker 15 m,
        quantized to 10 m, delta-encoded ("x y dx dy ...", rings ';',
        polygons '|').
    ma_ej_2020_towns.csv  The 351 cities and towns: 2020 Census population and
        housing units (MassGIS CENSUS2020TOWNS_POLY) and EEA's official
        municipal EJ statistics (November 2022 update) for the 187 towns with
        EJ block groups.
* cdc-heat-health-index-2024-ma
    ma_heat_health_index_2024.csv  CDC/ATSDR Heat and Health Index 2024
        release, Massachusetts ZCTAs: extreme heat days, tree canopy,
        impervious surface, and the module and overall percentile ranks
        (national). -999 (no data) is stored as blank.
* fema-nfip-ma
    fema_nfip_community_status_ma.csv  OpenFEMA NFIP Community Status Book
        (v1), every Massachusetts community.
    fema_nfip_claims_ma_towns.csv  OpenFEMA NFIP Redacted Claims (v3), every
        Massachusetts claim summed by city or town (community from the
        claim's NFIP rated community number, matched to the Community Status
        Book).
* fema-hazard-mitigation-plans-ma
    fema_hazard_mitigation_plans_ma.csv  OpenFEMA Hazard Mitigation Plan
        Statuses (v1), every Massachusetts row.
* ma-mvp-communities
    ma_mvp_communities.csv  Municipal Vulnerability Preparedness (MVP)
        program: MVP region of each town, the MVP planning report(s) listed
        for it on mass.gov, whether the town was designated through prior
        work, and whether it is in the FY24 MVP 2.0 pilot round.
"""

import csv
import datetime
import hashlib
import html
import http.client
import io
import json
import re
import struct
import sys
import time
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"

MASSGIS = "https://s3.us-east-1.amazonaws.com/download.massgis.digital.mass.gov/"
EJ_ZIP = MASSGIS + "shapefiles/census2020/ej2020.zip"
TOWNS_ZIP = MASSGIS + "shapefiles/census2020/CENSUS2020TOWNS_SHP.zip"
EJ_STATS_XLSX = MASSGIS + "shapefiles/census2020/EJ%202020%20updated%20municipal%20statistics%20Nov2022..xlsx"
HHI_ZIP = "https://www.atsdr.cdc.gov/place-health/media/files/2024/08/HHI_Data.zip"
OPENFEMA = "https://www.fema.gov/api/open/"
CSB_URL = OPENFEMA + "v1/NfipCommunityStatusBook?$filter=state%20eq%20%27MA%27&$top=1000"
HMP_URL = OPENFEMA + "v1/HazardMitigationPlanStatuses?$filter=stateAbbreviation%20eq%20%27MA%27&$top=1000"
CLAIMS_FIELDS = (
    "yearOfLoss,amountPaidOnBuildingClaim,amountPaidOnContentsClaim,"
    "amountPaidOnIncreasedCostOfComplianceClaim,nfipRatedCommunityNumber,nfipCommunityName,"
    "censusGeoid,id"
)
CLAIMS_URL = (
    OPENFEMA + "v3/NfipClaims?$filter=state%20eq%20%27MA%27&$select=" + CLAIMS_FIELDS
    + "&$orderby=id&$top=10000&$skip={skip}"
)
MVP_REGIONS = "https://www.mass.gov/info-details/contact-your-mvp-regional-coordinator"
MVP_REPORTS = "https://www.mass.gov/info-details/municipal-vulnerability-preparedness-mvp-program-planning-reports"
MVP_SNAPSHOTS = "https://www.mass.gov/lists/mvp-community-snapshots"

SIMPLIFY_M = 15.0
# Names on the MVP regions page that differ from the official town names.
REGION_ALIASES = {"Manchester": "Manchester-by-the-Sea"}


# ----------------------------------------------------------------- helpers

def get(url, attempts=3):
    # mass.gov refuses some custom user agents; the urllib default works everywhere used here.
    # Large downloads are retried when a transfer is cut off.
    for i in range(attempts):
        try:
            with urllib.request.urlopen(urllib.request.Request(url), timeout=600) as r:
                return r.read()
        except (http.client.IncompleteRead, ConnectionError, TimeoutError) as e:
            if i + 1 == attempts:
                raise
            print(f"retrying {url} after {e!r}", file=sys.stderr)
            time.sleep(5 * (i + 1))


def save(work, name, data):
    (work / name).write_bytes(data)
    return data


def sha(b):
    return hashlib.sha256(b).hexdigest()


def read_dbf(data):
    n = struct.unpack("<I", data[4:8])[0]
    hl = struct.unpack("<H", data[8:10])[0]
    rl = struct.unpack("<H", data[10:12])[0]
    fields, off = [], 32
    while data[off] != 0x0D:
        name = data[off:off + 11].split(b"\0")[0].decode()
        fields.append((name, data[off + 16]))
        off += 32
    rows = []
    for i in range(n):
        r = data[hl + i * rl: hl + (i + 1) * rl]
        if r[0:1] == b"*":
            continue
        p, d = 1, {}
        for name, ln in fields:
            d[name] = r[p:p + ln].decode("latin-1").strip()
            p += ln
        rows.append(d)
    return rows


def read_shp(data):
    """Polygon shapes (type 5) as lists of rings."""
    out, pos = [], 100
    while pos < len(data):
        _, clen = struct.unpack(">ii", data[pos:pos + 8])
        pos += 8
        c = data[pos:pos + clen * 2]
        pos += clen * 2
        st = struct.unpack("<i", c[:4])[0]
        if st == 0:
            out.append([])
            continue
        if st != 5:
            raise SystemExit(f"unexpected shape type {st}")
        nparts, npts = struct.unpack("<ii", c[36:44])
        parts = list(struct.unpack("<%di" % nparts, c[44:44 + 4 * nparts]))
        o = 44 + 4 * nparts
        pts = [struct.unpack("<dd", c[o + 16 * i:o + 16 * i + 16]) for i in range(npts)]
        rings = []
        for i, s in enumerate(parts):
            e = parts[i + 1] if i + 1 < nparts else npts
            rings.append(pts[s:e])
        out.append(rings)
    return out


def ring_area(r):
    a = 0.0
    for i in range(len(r)):
        x1, y1 = r[i]
        x2, y2 = r[(i + 1) % len(r)]
        a += x1 * y2 - x2 * y1
    return a / 2


def simplify(pts, tol):
    """Douglas-Peucker."""
    if len(pts) < 3:
        return pts
    keep = [False] * len(pts)
    keep[0] = keep[-1] = True
    stack = [(0, len(pts) - 1)]
    while stack:
        s, e = stack.pop()
        (x1, y1), (x2, y2) = pts[s], pts[e]
        dx, dy = x2 - x1, y2 - y1
        length = (dx * dx + dy * dy) ** 0.5
        best, bi = -1.0, -1
        for i in range(s + 1, e):
            x, y = pts[i]
            if length > 0:
                d = abs(dy * x - dx * y + x2 * y1 - y2 * x1) / length
            else:
                d = ((x - x1) ** 2 + (y - y1) ** 2) ** 0.5
            if d > best:
                best, bi = d, i
        if best > tol:
            keep[bi] = True
            stack.append((s, bi))
            stack.append((bi, e))
    return [p for p, k in zip(pts, keep) if k]


def encode_geometry(rings):
    """Exterior rings start polygons; holes (clockwise in shapefiles is exterior) follow."""
    polys = []
    for ring in rings:
        if abs(ring_area(ring)) < 4 * SIMPLIFY_M * SIMPLIFY_M:
            continue
        s = simplify(ring, SIMPLIFY_M)
        q = []
        for x, y in s:
            p = (int(round(x / 10.0)), int(round(y / 10.0)))
            if not q or q[-1] != p:
                q.append(p)
        if len(q) > 1 and q[0] == q[-1]:
            q.pop()
        if len(q) < 3:
            continue
        nums = [q[0][0], q[0][1]]
        for (ax, ay), (bx, by) in zip(q, q[1:]):
            nums += [bx - ax, by - ay]
        enc = " ".join(str(n) for n in nums)
        # Shapefile exterior rings are clockwise (negative signed area).
        if ring_area(ring) < 0 or not polys:
            polys.append([enc])
        else:
            polys[-1].append(enc)
    return "|".join(";".join(p) for p in polys)


def read_xlsx(data, sheet=1):
    import xml.etree.ElementTree as ET
    ns = "{http://schemas.openxmlformats.org/spreadsheetml/2006/main}"
    z = zipfile.ZipFile(io.BytesIO(data))
    strings = []
    if "xl/sharedStrings.xml" in z.namelist():
        for si in ET.fromstring(z.read("xl/sharedStrings.xml")).findall(ns + "si"):
            strings.append("".join(t.text or "" for t in si.iter(ns + "t")))
    rows = []
    for r in ET.fromstring(z.read(f"xl/worksheets/sheet{sheet}.xml")).iter(ns + "row"):
        row = {}
        for c in r.findall(ns + "c"):
            ref = re.match(r"[A-Z]+", c.get("r")).group(0)
            idx = 0
            for ch in ref:
                idx = idx * 26 + ord(ch) - 64
            v = c.find(ns + "v")
            t = c.get("t")
            if t == "inlineStr":
                val = "".join(x.text or "" for x in c.iter(ns + "t"))
            elif v is None:
                val = ""
            elif t == "s":
                val = strings[int(v.text)]
            else:
                val = v.text
            row[idx - 1] = val
        if row:
            rows.append([row.get(i, "") for i in range(max(row) + 1)])
    return rows


def num(s, places):
    s = (s or "").strip()
    if s == "":
        return ""
    v = float(s)
    if v == -999:
        return ""
    if places == 0:
        return str(int(round(v)))
    out = f"{v:.{places}f}".rstrip("0").rstrip(".")
    return "0" if out in ("-0", "") else out


def to_csv(header, rows):
    buf = io.StringIO()
    w = csv.writer(buf, lineterminator="\n")
    w.writerow(header)
    w.writerows(rows)
    return buf.getvalue()


def norm(s):
    return re.sub(r"[^a-z]", "", s.lower())


def towns_list():
    with open(SNAP / "ma_municipalities.csv", newline="") as f:
        return [(r["name"], r["county"]) for r in csv.DictReader(f)]


def whole_word(name):
    return re.compile(r"(?<![A-Za-z])" + re.escape(name) + r"(?![A-Za-z])")


def towns_in_text(text, towns):
    """Town names that appear in `text` as whole words. A name inside a longer
    matched name ("Boylston" in "West Boylston") counts only if it also
    appears on its own."""
    found = [t for t in towns if whole_word(t).search(text)]
    out = []
    for t in found:
        rest = text
        for o in found:
            if o != t and t in o:
                rest = whole_word(o).sub(" ", rest)
        if whole_word(t).search(rest):
            out.append(t)
    return out


def page_text(html_bytes):
    d = html_bytes.decode("utf-8", "replace")
    m = re.search(r"<main.*?</main>", d, re.S)
    d = m.group(0) if m else d
    d = re.sub(r"<script.*?</script>", "", d, flags=re.S)
    d = re.sub(r"<style.*?</style>", "", d, flags=re.S)
    t = re.sub(r"<[^>]+>", "\n", d)
    t = html.unescape(t)
    return [ln.strip() for ln in t.splitlines() if ln.strip()]


# ----------------------------------------------------------------- builders

def ej(work, sources):
    ejz = save(work, "ej2020.zip", get(EJ_ZIP))
    tz = save(work, "CENSUS2020TOWNS_SHP.zip", get(TOWNS_ZIP))
    xl = save(work, "ej2020_municipal_stats.xlsx", get(EJ_STATS_XLSX))
    sources["ma-ej-2020"] = {
        "ej2020.zip": sha(ejz),
        "CENSUS2020TOWNS_SHP.zip": sha(tz),
        "EJ 2020 updated municipal statistics Nov2022..xlsx": sha(xl),
    }
    z = zipfile.ZipFile(io.BytesIO(ejz))
    attrs = read_dbf(z.read("EJ_POLY.dbf"))
    shapes = read_shp(z.read("EJ_POLY.shp"))
    if len(attrs) != len(shapes):
        raise SystemExit("EJ_POLY: attribute and shape counts differ")
    towns = {n for n, _ in towns_list()}
    rows = []
    for a, shp in zip(attrs, shapes):
        town = a["MUNICIPALI"]
        if town not in towns:
            raise SystemExit(f"EJ_POLY: unknown municipality {town}")
        if a["EJ"] != "Yes":
            continue
        crit = a["EJ_CRITERI"]
        if not crit or set(crit) - set("MIE"):
            raise SystemExit(f"EJ_POLY: odd criteria {crit!r}")
        rows.append([
            a["GEOID"],
            town,
            "Y" if a["SPLIT_FOR_"].upper() == "Y" else "N",
            num(a["TOTAL_POP"], 0),
            num(a["PCT_MINORI"], 1),
            num(a["TOTALHH"], 0),
            num(a["LIMENGHH"], 0),
            num(a["LIMENGHHPC"], 1),
            num(a["BG_MHHI"], 0),
            num(a["BG_MHHI_PC"], 1),
            "1" if a["BG_MHHI_IS"] == "1" else "0",
            num(a["MUNIMHHI_P"], 1),
            crit,
            encode_geometry(shp),
        ])
    rows.sort(key=lambda r: (r[0], r[1]))
    bg = to_csv(
        ["geoid", "town", "split_for_town", "population", "pct_minority", "households",
         "limited_english_households", "pct_limited_english_households", "bg_median_household_income",
         "bg_mhhi_pct_of_state", "mhhi_from_2019", "town_mhhi_pct_of_state", "criteria", "geometry"],
        rows,
    )

    tzf = zipfile.ZipFile(io.BytesIO(tz))
    census = {r["GEOID20"]: r for r in read_dbf(tzf.read("CENSUS2020TOWNS_POLY.dbf"))}
    geoid_of = {}
    with open(SNAP / "ma_census_geographies.csv", newline="") as f:
        for r in csv.DictReader(f):
            if r["level"] == "town":
                geoid_of[r["name"]] = r["geoid"]
    stats = {}
    sheet = read_xlsx(xl)
    if sheet[0][1:9] != ["Municipality", "EJ criteria", "Number of EJ block groups",
                         "Total number of block groups", "Percent of EJ block groups",
                         "Population in EJ block groups", "Total population",
                         "Percent of population in EJ BGs"]:
        raise SystemExit(f"municipal statistics: unexpected header {sheet[0]}")
    for r in sheet[1:]:
        if len(r) > 1 and r[1]:
            if r[1] not in towns:
                raise SystemExit(f"municipal statistics: unknown town {r[1]}")
            stats[r[1]] = r
    trows = []
    for name, county in towns_list():
        g = geoid_of[name]
        c = census.get(g)
        if c is None:
            raise SystemExit(f"no 2020 census row for {name} ({g})")
        s = stats.get(name)
        trows.append([
            name, g, county, num(c["POP2020"], 0), num(c["HOUSING20"], 0),
            s[2] if s else "",
            num(s[3], 0) if s else "",
            num(s[4], 0) if s else "",
            num(s[6], 0) if s else "",
            num(s[7], 0) if s else "",
            num(s[8], 1) if s else "",
        ])
    tw = to_csv(
        ["town", "geoid", "county", "pop_2020", "housing_units_2020", "ej_criteria",
         "ej_block_groups", "total_block_groups", "pop_in_ej_block_groups", "stats_total_pop",
         "pct_pop_in_ej_block_groups"],
        trows,
    )
    return {"ma_ej_2020_block_groups.csv": bg, "ma_ej_2020_towns.csv": tw}


HHI_KEEP = [
    ("ZCTA", "zcta", None), ("POP", "population", 0), ("P_NEHD", "extreme_heat_days", 2),
    ("PR_NEHD", "pr_extreme_heat_days", 4), ("PR_HRI", "pr_heat_ems", 4),
    ("HHB_RANK", "rank_heat_burden", 4), ("SEN_RANK", "rank_sensitivity", 4),
    ("SOCIODEM_RANK", "rank_sociodemographic", 4), ("NBE_RANK", "rank_built_environment", 4),
    ("OVERALL_RANK", "rank_overall", 4), ("P_IMPERV", "pct_impervious", 2),
    ("PR_IMPERV", "pr_impervious", 4), ("P_TREEC", "pct_tree_canopy", 2),
    ("PR_TREEC", "pr_tree_canopy", 4),
]


def hhi(work, sources):
    data = save(work, "HHI_Data.zip", get(HHI_ZIP))
    sources["cdc-heat-health-index-2024-ma"] = {"HHI_Data.zip": sha(data)}
    z = zipfile.ZipFile(io.BytesIO(data))
    rows = read_xlsx(z.read("HHI Data 2024 United States.xlsx"))
    head = rows[0]
    idx = {h: i for i, h in enumerate(head)}
    for h, _, _ in HHI_KEEP:
        if h not in idx:
            raise SystemExit(f"HHI: missing column {h}")
    out = []
    for r in rows[1:]:
        if r[idx["STATEFP10"]] != "25":
            continue
        out.append([
            r[idx[h]].zfill(5) if p is None else num(r[idx[h]], p) for h, _, p in HHI_KEEP
        ])
    out.sort()
    return {"ma_heat_health_index_2024.csv": to_csv([n for _, n, _ in HHI_KEEP], out)}


CSB_KEEP = [
    "communityIdNumber", "communityName", "county", "participatingInNFIP",
    "currentlyEffectiveMapDate", "regularEmergencyProgramDate", "classRating",
    "classRatingEffectiveDate", "sfhaDiscount", "sanction", "sanctionDate", "tribal",
]
HMP_KEEP = [
    "planId", "mppGeoid", "placeName", "countyName", "jurisdictionType", "communityIdNumber",
    "planTitle", "planType", "planStatus", "jurisdictionStatus", "planApprovalDate",
    "planExpirationDate", "adoptionDate", "jurisdictionApprovalDate",
]


def cell(v):
    if v is None:
        return ""
    if isinstance(v, bool):
        return "true" if v else "false"
    s = str(v)
    # Dates as YYYY-MM-DD.
    if re.match(r"^\d{4}-\d\d-\d\dT00:00:00(\.000)?Z$", s):
        return s[:10]
    return s


def community_towns(csb):
    """NFIP community number -> town name."""
    by_norm = {norm(n): n for n, _ in towns_list()}
    out = {}
    for r in csb:
        base = re.split(r"\s*,", r["communityName"])[0].strip()
        t = by_norm.get(norm(base))
        if t is None:
            raise SystemExit(f"Community Status Book: no town for {r['communityName']}")
        out[r["communityIdNumber"]] = t
    return out


def fema(work, sources):
    csb_raw = save(work, "nfip_community_status_ma.json", get(CSB_URL))
    hmp_raw = save(work, "hazard_mitigation_plans_ma.json", get(HMP_URL))
    csb = json.loads(csb_raw)["NfipCommunityStatusBook"]
    hmp = json.loads(hmp_raw)["HazardMitigationPlanStatuses"]
    if len(csb) >= 1000 or len(hmp) >= 1000:
        raise SystemExit("OpenFEMA: more than 1000 rows; page the query")
    claims, skip, raw_hashes = [], 0, hashlib.sha256()
    while True:
        b = get(CLAIMS_URL.format(skip=skip))
        raw_hashes.update(b)
        page = json.loads(b)["NfipClaims"]
        claims += page
        if len(page) < 10000:
            break
        skip += 10000
    (work / "nfip_claims_ma.json").write_text(json.dumps(claims))
    sources["fema-nfip-ma"] = {
        "NfipCommunityStatusBook (MA, JSON)": sha(csb_raw),
        "NfipClaims v3 (MA, JSON pages)": raw_hashes.hexdigest(),
    }
    sources["fema-hazard-mitigation-plans-ma"] = {
        "HazardMitigationPlanStatuses (MA, JSON)": sha(hmp_raw),
    }

    csb_rows = sorted(([cell(r.get(k)) for k in CSB_KEEP] for r in csb), key=lambda r: r[0])
    hmp_rows = sorted(
        ([cell(r.get(k)) for k in HMP_KEEP] for r in hmp),
        key=lambda r: (r[1], r[10] or "", r[0]),
    )

    # Claims by town.
    ctown = community_towns(csb)
    by_norm = {norm(n): n for n, _ in towns_list()}
    tract_town = {}
    with open(SNAP / "ma_census_geographies.csv", newline="") as f:
        for r in csv.DictReader(f):
            if r["level"] == "tract" and r["town"]:
                tract_town[r["geoid"]] = r["town"]
    retrieved = datetime.date.today()
    recent_to = retrieved.year - 1
    recent_from = recent_to - 9
    agg = {}
    unmatched = 0
    method = {"community": 0, "name": 0, "tract": 0}
    for c in claims:
        t = ctown.get(c.get("nfipRatedCommunityNumber") or "")
        if t:
            method["community"] += 1
        if t is None and c.get("nfipCommunityName"):
            t = by_norm.get(norm(re.split(r"\s*,", c["nfipCommunityName"])[0]))
            if t:
                method["name"] += 1
        if t is None and c.get("censusGeoid"):
            t = tract_town.get(c["censusGeoid"][:11])
            if t:
                method["tract"] += 1
        if t is None:
            unmatched += 1
            continue
        paid = sum(float(c.get(k) or 0) for k in (
            "amountPaidOnBuildingClaim", "amountPaidOnContentsClaim",
            "amountPaidOnIncreasedCostOfComplianceClaim"))
        y = int(c["yearOfLoss"])
        a = agg.setdefault(t, {"claims": 0, "paid_claims": 0, "paid": 0.0, "recent": 0,
                               "recent_paid": 0.0, "years": {}, "first": y})
        a["claims"] += 1
        a["paid"] += paid
        if paid > 0:
            a["paid_claims"] += 1
        if recent_from <= y <= recent_to:
            a["recent"] += 1
            a["recent_paid"] += paid
        a["years"][y] = a["years"].get(y, 0) + 1
        a["first"] = min(a["first"], y)
    crow = []
    for name, _ in towns_list():
        a = agg.get(name)
        if a is None:
            crow.append([name, 0, 0, 0, 0, 0, "", "", "", recent_from, recent_to])
            continue
        top = max(a["years"].items(), key=lambda kv: (kv[1], -kv[0]))
        crow.append([name, a["claims"], a["paid_claims"], int(round(a["paid"])), a["recent"],
                     int(round(a["recent_paid"])), a["first"], top[0], top[1], recent_from, recent_to])
    print(f"NFIP claims: {len(claims)} total, matched by community {method['community']}, "
          f"by name {method['name']}, by tract {method['tract']}, unmatched {unmatched}",
          file=sys.stderr)
    nfip_note = {"claims": len(claims), "unmatched": unmatched, **method}
    return {
        "fema_nfip_community_status_ma.csv": to_csv(CSB_KEEP, csb_rows),
        "fema_nfip_claims_ma_towns.csv": to_csv(
            ["town", "claims", "paid_claims", "total_paid", "claims_recent", "paid_recent",
             "first_year", "peak_year", "peak_year_claims", "recent_from", "recent_to"],
            crow,
        ),
        "fema_hazard_mitigation_plans_ma.csv": to_csv(HMP_KEEP, hmp_rows),
    }, nfip_note


def mvp(work, sources):
    regions_html = save(work, "mvp_regions.html", get(MVP_REGIONS))
    reports_html = save(work, "mvp_reports.html", get(MVP_REPORTS))
    snaps_html = save(work, "mvp_snapshots.html", get(MVP_SNAPSHOTS))
    sources["ma-mvp-communities"] = {
        "contact-your-mvp-regional-coordinator.html": sha(regions_html),
        "municipal-vulnerability-preparedness-mvp-program-planning-reports.html": sha(reports_html),
        "mvp-community-snapshots.html": sha(snaps_html),
    }
    towns = [n for n, _ in towns_list()]
    tset = set(towns)
    # Regions: "Cities and Towns in the X Region" followed by town names.
    region = {}
    cur = None
    for ln in page_text(regions_html):
        m = re.match(r"^Cities and Towns in the (.+ Region)$", ln)
        if m:
            cur = m.group(1)
            continue
        if ln.endswith(" Region") or ln.startswith("MVP Coordinator"):
            cur = None
            continue
        name = REGION_ALIASES.get(ln, ln)
        if cur and name in tset:
            region[name] = cur
    # Planning reports: link texts; a town counts when its name appears as a whole word.
    d = reports_html.decode("utf-8", "replace")
    m = re.search(r"<main.*?</main>", d, re.S)
    d = m.group(0) if m else d
    link_re = r'<a [^>]*href="/doc/[^"]*"[^>]*>((?:(?!</a>).)*)</a>'
    links = [html.unescape(re.sub(r"<[^>]+>", "", t)).strip() for t in re.findall(link_re, d, re.S)]
    titles = {t: [] for t in towns}
    starred = set()
    for title in links:
        # One entry per report: skip appendices and later parts.
        if re.search(r"(?i)\bappendix\b", title) or re.search(r"(?i)\(part [2-9]", title):
            continue
        for t in towns_in_text(title, towns):
            if title not in titles[t]:
                titles[t].append(title)
    # "*Designated MVP community through prior work": the asterisk follows the link.
    for m2 in re.finditer(link_re + r"\s*(?:</?[a-z]+>\s*|&nbsp;\s*)*\*", d, re.S):
        title = html.unescape(re.sub(r"<[^>]+>", "", m2.group(1))).strip()
        for t in towns:
            if title in titles[t]:
                starred.add(t)
    pilot = set()
    in_list = False
    for ln in page_text(snaps_html):
        if ln.startswith("You skipped the table of contents"):
            in_list = True
            continue
        if ln.startswith("Help Us Improve"):
            break
        if in_list and ln in tset:
            pilot.add(ln)
    rows = []
    for t in towns:
        rows.append([
            t,
            region.get(t, ""),
            "; ".join(titles[t]),
            "true" if t in starred else "false",
            "true" if t in pilot else "false",
        ])
    missing_region = [t for t in towns if t not in region]
    if missing_region:
        print(f"MVP: no region for {missing_region}", file=sys.stderr)
    return {"ma_mvp_communities.csv": to_csv(
        ["town", "mvp_region", "planning_reports", "designated_through_prior_work",
         "mvp2_fy24_pilot"], rows)}


# ----------------------------------------------------------------- main

def file_entry(name, text):
    b = text.encode("utf-8")
    return {"path": name, "sha256": sha(b), "bytes": len(b), "rows": text.count("\n") - 1}


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if len(args) != 1:
        raise SystemExit(__doc__)
    check = "--check" in sys.argv
    work = Path(args[0]).resolve()
    if ROOT in work.parents or work == ROOT:
        raise SystemExit("WORK_DIR must be outside the repository")
    work.mkdir(parents=True, exist_ok=True)
    sources = {}
    files = {}
    files.update(ej(work, sources))
    files.update(hhi(work, sources))
    f, nfip_note = fema(work, sources)
    files.update(f)
    files.update(mvp(work, sources))
    changed = []
    for name, text in files.items():
        p = SNAP / name
        old = p.read_text(encoding="utf-8") if p.exists() else None
        if old != text:
            changed.append(name)
            if not check:
                p.write_text(text, encoding="utf-8", newline="\n")
    print("changed: " + (", ".join(changed) if changed else "none"))
    groups = {
        "ma-ej-2020": ["ma_ej_2020_block_groups.csv", "ma_ej_2020_towns.csv"],
        "cdc-heat-health-index-2024-ma": ["ma_heat_health_index_2024.csv"],
        "fema-nfip-ma": ["fema_nfip_community_status_ma.csv", "fema_nfip_claims_ma_towns.csv"],
        "fema-hazard-mitigation-plans-ma": ["fema_hazard_mitigation_plans_ma.csv"],
        "ma-mvp-communities": ["ma_mvp_communities.csv"],
    }
    out = {
        k: {"source_sha256": sources[k], "files": [file_entry(n, files[n]) for n in v]}
        for k, v in groups.items()
    }
    out["fema-nfip-ma"]["claims_matching"] = nfip_note
    out["retrieved"] = datetime.date.today().isoformat()
    print(json.dumps(out, indent=2))
    if check and changed:
        sys.exit(1)


if __name__ == "__main__":
    main()
