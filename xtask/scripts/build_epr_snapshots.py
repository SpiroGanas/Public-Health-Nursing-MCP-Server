#!/usr/bin/env python3
"""Build the Phase 12 (emergency preparedness) snapshots.

Usage (from the repository root, Python 3.9+, standard library only):

    python3 -I xtask/scripts/build_epr_snapshots.py WORK_DIR [--check]

WORK_DIR is an empty scratch folder for downloads (never the repository). The
script downloads the sources, writes the snapshot files to snapshots/, and
prints the manifest fields (source checksums, row counts, file checksums) to
paste into snapshots/manifest.json. With --check it writes nothing and reports
whether the files would change.

Snapshots:

* fema_declarations_ma.csv  OpenFEMA DisasterDeclarationsSummaries v2, every
  Massachusetts row (one per declaration and designated area).
* empower_ma.csv            HHS emPOWER Map JSON API: Massachusetts total,
  counties, and ZIP codes (Medicare beneficiaries and electricity-dependent
  DME users; values of 11 mean 1-11 under the CMS cell-size policy).
* ma_pl2020_blocks.txt      2020 Census Redistricting Data (PL 94-171) for
  Massachusetts: every census block with at least one housing unit, with total
  and vacant housing units (table H1), for CASPER cluster sampling.

Block file format (one line per county, county subdivision, and tract):
    COUNTY,COUSUB,TRACT,BLOCK:HU[:VACANT] BLOCK:HU[:VACANT] ...
VACANT is omitted when zero. HU = H1_001N (all housing units), VACANT =
H1_003N, so occupied = HU - VACANT (H1_002N).
"""

import csv
import hashlib
import io
import json
import sys
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
UA = "phn-mcp-server snapshot builder (+https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server)"

FEMA_URL = (
    "https://www.fema.gov/api/open/v2/DisasterDeclarationsSummaries?$filter=state%20eq%20%27MA%27"
    "&$orderby=declarationDate%20desc,disasterNumber%20desc&$top=5000&$select=disasterNumber,"
    "declarationType,declarationDate,incidentType,declarationTitle,designatedArea,fipsCountyCode,"
    "incidentBeginDate,incidentEndDate,ihProgramDeclared,iaProgramDeclared,paProgramDeclared,"
    "hmProgramDeclared,femaDeclarationString,lastRefresh"
)
EMPOWER = "https://empowerprogram.hhs.gov/api/v1/dme"
PL_URL = (
    "https://www2.census.gov/programs-surveys/decennial/2020/data/"
    "01-Redistricting_File--PL_94-171/Massachusetts/ma2020.pl.zip"
)

GEO_HEADER = (
    "FILEID|STUSAB|SUMLEV|GEOVAR|GEOCOMP|CHARITER|CIFSN|LOGRECNO|GEOID|GEOCODE|REGION|DIVISION|"
    "STATE|STATENS|COUNTY|COUNTYCC|COUNTYNS|COUSUB|COUSUBCC|COUSUBNS|SUBMCD|SUBMCDCC|SUBMCDNS|"
    "ESTATE|ESTATECC|ESTATENS|CONCIT|CONCITCC|CONCITNS|PLACE|PLACECC|PLACENS|TRACT|BLKGRP|BLOCK|"
    "AIANHH|AIHHTLI|AIANHHFP|AIANHHCC|AIANHHNS|AITS|AITSFP|AITSCC|AITSNS|TTRACT|TBLKGRP|ANRC|"
    "ANRCCC|ANRCNS|CBSA|MEMI|CSA|METDIV|NECTA|NMEMI|CNECTA|NECTADIV|CBSAPCI|NECTAPCI|UA|UATYPE|"
    "UR|CD116|CD118|CD119|CD120|CD121|SLDU18|SLDU22|SLDU24|SLDU26|SLDU28|SLDL18|SLDL22|SLDL24|"
    "SLDL26|SLDL28|VTD|VTDI|ZCTA|SDELM|SDSEC|SDUNI|PUMA|AREALAND|AREAWATR|BASENAME|NAME|FUNCSTAT|"
    "GCUNI|POP100|HU100|INTPTLAT|INTPTLON|LSADC|PARTFLAG|UGA"
).split("|")


def get(url, dest):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=300) as r:
        data = r.read()
    dest.write_bytes(data)
    return data


def sha(b):
    return hashlib.sha256(b).hexdigest()


def num(s):
    return int(str(s).replace(",", ""))


def fema(work):
    raw = get(FEMA_URL, work / "fema_ma.json")
    rows = json.loads(raw)["DisasterDeclarationsSummaries"]
    out = io.StringIO()
    w = csv.writer(out, lineterminator="\n")
    w.writerow(["id", "disaster_number", "declaration_type", "declaration_date", "incident_type",
                "title", "designated_area", "fips_county_code", "incident_begin", "incident_end",
                "programs"])
    refreshed = max(r["lastRefresh"][:10] for r in rows)
    for r in rows:
        progs = [p for k, p in [("ihProgramDeclared", "IH"), ("iaProgramDeclared", "IA"),
                                ("paProgramDeclared", "PA"), ("hmProgramDeclared", "HM")] if r[k]]
        w.writerow([r["femaDeclarationString"], r["disasterNumber"], r["declarationType"],
                    r["declarationDate"][:10], r["incidentType"], r["declarationTitle"],
                    r["designatedArea"], r["fipsCountyCode"],
                    (r["incidentBeginDate"] or "")[:10], (r["incidentEndDate"] or "")[:10],
                    ";".join(progs)])
    return out.getvalue(), {"fema_ma.json": sha(raw)}, len(rows), refreshed


def empower(work):
    nat = json.loads(get(f"{EMPOWER}/national.json", work / "empower_national.json"))
    states = json.loads(get(f"{EMPOWER}/states.json", work / "empower_states.json"))
    counties_raw = get(f"{EMPOWER}/states/Massachusetts/counties.json", work / "empower_counties.json")
    zips_raw = get(f"{EMPOWER}/states/Massachusetts/zipcodes.json", work / "empower_zips.json")
    counties = json.loads(counties_raw)
    zips = json.loads(zips_raw)
    ma = next(s for s in states if s["abbr"] == "MA")
    out = io.StringIO()
    w = csv.writer(out, lineterminator="\n")
    w.writerow(["level", "geoid", "name", "county", "medicare_beneficiaries", "electricity_dependent"])
    w.writerow(["nation", "US", "United States", "", num(nat["benes"]), num(nat["dme"])])
    w.writerow(["state", "25", "Massachusetts", "", num(ma["benes"]), num(ma["dme"])])
    for c in sorted(counties, key=lambda c: c["countyFips"]):
        w.writerow(["county", c["countyFips"], c["area"], c["area"], num(c["benes"]), num(c["dme"])])
    for z in sorted(zips, key=lambda z: z["area"]):
        w.writerow(["zip", z["area"], z["area"], z.get("parent", ""), num(z["benes"]), num(z["dme"])])
    rows = 2 + len(counties) + len(zips)
    return out.getvalue(), {
        "empower_counties.json": sha(counties_raw),
        "empower_zips.json": sha(zips_raw),
    }, rows


def pl_blocks(work):
    raw = get(PL_URL, work / "ma2020.pl.zip")
    z = zipfile.ZipFile(io.BytesIO(raw))
    ix = {h: i for i, h in enumerate(GEO_HEADER)}
    blocks = {}
    with z.open("mageo2020.pl") as f:
        for line in io.TextIOWrapper(f, encoding="latin-1"):
            p = line.rstrip("\n").split("|")
            if p[ix["SUMLEV"]] != "750":
                continue
            hu = int(p[ix["HU100"]])
            if hu == 0:
                continue
            blocks[p[ix["LOGRECNO"]]] = (p[ix["COUNTY"]], p[ix["COUSUB"]], p[ix["TRACT"]],
                                         p[ix["BLOCK"]], hu)
    groups = {}
    total = 0
    with z.open("ma000022020.pl") as f:
        for line in io.TextIOWrapper(f, encoding="latin-1"):
            p = line.rstrip("\n").split("|")
            rec = blocks.get(p[4])
            if rec is None:
                continue
            h1, h2, h3 = int(p[-3]), int(p[-2]), int(p[-1])
            county, cousub, tract, block, hu = rec
            if h1 != hu or h1 != h2 + h3:
                raise SystemExit(f"housing unit mismatch in block {county}{tract}{block}")
            total += h1
            entry = f"{block}:{h1}" + (f":{h3}" if h3 else "")
            groups.setdefault((county, cousub, tract), []).append(entry)
    lines = [f"{c},{s},{t},{' '.join(v)}" for (c, s, t), v in sorted(groups.items())]
    text = "\n".join(lines) + "\n"
    return text, {"ma2020.pl.zip": sha(raw)}, len(lines), (len(blocks), total)


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    work = Path(sys.argv[1])
    work.mkdir(parents=True, exist_ok=True)
    check = "--check" in sys.argv
    results = {}
    text, src, rows, refreshed = fema(work)
    results["fema_declarations_ma.csv"] = (text, src, rows, f"OpenFEMA refresh {refreshed}")
    text, src, rows = empower(work)
    results["empower_ma.csv"] = (text, src, rows, "")
    text, src, rows, (nblocks, hu) = pl_blocks(work)
    results["ma_pl2020_blocks.txt"] = (text, src, rows, f"{nblocks} blocks, {hu} housing units")
    for name, (text, src, rows, note) in results.items():
        data = text.encode("utf-8")
        path = SNAP / name
        changed = not path.exists() or path.read_bytes() != data
        if not check:
            path.write_bytes(data)
        print(json.dumps({"path": name, "sha256": sha(data), "bytes": len(data), "rows": rows,
                          "source_sha256": src, "note": note, "changed": changed}, indent=1))


if __name__ == "__main__":
    main()
