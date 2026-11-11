#!/usr/bin/env python3
"""Build the Phase 14 water snapshots (drinking water quality and fluoridation).

Usage (from the repository root, Python 3.9+, standard library only):

    python3 -I xtask/scripts/build_water_snapshots.py WORK_DIR [--check]

WORK_DIR is an empty scratch folder for downloads (never the repository). The
script downloads the sources, writes the snapshot files to snapshots/, and
prints the manifest fields (source checksums, row counts, file checksums) to
paste into snapshots/manifest.json. With --check it writes nothing and reports
whether the files would change.

Snapshots (all public data; no person's name, email, or phone is kept):

* ma_water_systems.csv       EPA SDWIS (Envirofacts WATER_SYSTEM): every ACTIVE
  public water system whose primacy agency is Massachusetts, plus tribal
  systems in Massachusetts (primacy EPA Region 1). Kept: PWSID, system name,
  type (CWS, NTNCWS, TNCWS), town, population served, service connections,
  primary source, owner type, wholesaler and school/child care flags. The town
  is read from the PWSID (MA + MassDEP region digit + three-digit town number
  + system number; the town number is the alphabetical MassGIS town id); for a
  tribal system it is the town named in the system name. Operator names,
  emails, phone numbers, and addresses are dropped.
* ma_water_violations.csv    EPA SDWIS VIOLATION rows for those systems whose
  compliance period began on or after 2016-01-01.
* ma_water_lead_copper.csv   EPA SDWIS LCR_SAMPLE_RESULT joined to LCR_SAMPLE
  (by PWSID and sample id): 90th percentile lead (PB90) and copper (CU90) by
  monitoring period ending on or after 2016-01-01.
* sdwis_codes.csv            EPA SDWIS reference code values (violation,
  violation category, contaminant, rule, source, system type, owner type, and
  compliance status codes) from sdwis.ref_code_value.
* ma_water_pfas6.csv         MassDEP PFAS6 results for finished (treated)
  water from the EEA Data Portal drinking water API, collected in the 24
  months before the build date: PWSID, date, sampling location, result (ng/L
  or ND).
* ma_fluoridation_systems.csv CDC My Water's Fluoride (Water Fluoridation
  Reporting System, state-reported): the Massachusetts Fluoridation Status
  Report for the latest year (every community water system with population,
  fluoridated yes/no, status, water source), each matched to a SDWIS PWSID and
  a town, plus the annual average fluoride level reported for adjusted systems
  (Average Fluoride Levels by Month report).
* ma_fluoridation_summary.csv CDC My Water's Fluoride Monthly Summary Report for
  Massachusetts (latest month): systems and population by fluoridation status.
* ma_fluoridation_towns.csv  DPH Office of Oral Health, 2023 list of
  Massachusetts cities and towns by community water fluoridation status (CSV
  linked from mass.gov "Community Water Fluoridation Status").
"""

import csv
import datetime as dt
import hashlib
import html
import http.cookiejar
import io
import json
import re
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
UA = "phn-mcp-server snapshot builder (+https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server)"

EF = "https://data.epa.gov/efservice"
DMAP = "https://data.epa.gov/dmapservice"
EEA = "https://eeaonline.eea.state.ma.us/EEA/DataLake/V1.0/DataLakeAPI/drinkingWater"
MWF = "https://nccd.cdc.gov/doh_mwf"
DPH_CSV = ("https://www.mass.gov/files/csv/2024-01/"
           "cities-and-towns-fluoride-population-2023-update%20-%20Copy.csv")
SINCE = "2016-01-01"
CODE_TYPES = ["VIOLATION_CODE", "VIOLATION_CATEGORY_CODE", "CONTAMINANT_CODE", "RULE_CODE",
              "PRIMARY_SOURCE_CODE", "PWS_TYPE_CODE", "OWNER_TYPE_CODE", "COMPLIANCE_STATUS_CODE"]

OPENER = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))


def get(url, dest=None, data=None, tries=4):
    last = None
    for i in range(tries):
        try:
            req = urllib.request.Request(url, data=data, headers={"User-Agent": UA})
            with OPENER.open(req, timeout=600) as r:
                body = r.read()
            if dest is not None:
                dest.write_bytes(body)
            return body
        except Exception as e:  # the CDC server sometimes drops a connection
            last = e
            time.sleep(5 * (i + 1))
    raise SystemExit(f"download failed: {url}: {last}")


def sha(b):
    return hashlib.sha256(b).hexdigest()


def rows_of(raw):
    return list(csv.DictReader(io.StringIO(raw.decode("utf-8-sig"))))


def csv_text(header, rows):
    out = io.StringIO()
    w = csv.writer(out, lineterminator="\n")
    w.writerow(header)
    w.writerows(rows)
    return out.getvalue()


def norm(s):
    return re.sub(r"[^a-z0-9]+", " ", s.lower()).strip()


def load_towns():
    towns = {}
    with open(SNAP / "ma_municipalities.csv", encoding="utf-8") as f:
        for r in csv.DictReader(f):
            towns[int(r["town_id"])] = r["name"]
    return towns


def town_in_name(name, towns):
    """A town whose name appears in `name` as whole words (longest first)."""
    n = " " + norm(name) + " "
    for t in sorted(towns.values(), key=len, reverse=True):
        if " " + norm(t) + " " in n:
            return t
    return None


def pws_town(pwsid, towns):
    if pwsid.startswith("MA") and len(pwsid) == 9 and pwsid[2:].isdigit():
        return towns.get(int(pwsid[3:6]))
    return None


def num(s):
    s = (s or "").strip()
    if not s:
        return ""
    try:
        v = float(s)
    except ValueError:
        return ""
    return str(int(v)) if v == int(v) else repr(v)


# --------------------------------------------------------------------- SDWIS


def sdwis(work, towns):
    src = {}

    def fetch(name, url):
        raw = get(url, work / name)
        src[name] = sha(raw)
        return rows_of(raw)

    ws_ma = fetch("water_system_ma.csv", f"{EF}/WATER_SYSTEM/PRIMACY_AGENCY_CODE/MA/CSV")
    ws_01 = fetch("water_system_01.csv", f"{EF}/WATER_SYSTEM/PRIMACY_AGENCY_CODE/01/CSV")
    tribal = [r for r in ws_01 if r["state_code"] == "MA"]
    systems = {}
    for r in ws_ma + tribal:
        if r["pws_activity_code"] != "A":
            continue
        p = r["pwsid"]
        town = pws_town(p, towns) or town_in_name(r["pws_name"], towns) or ""
        systems[p] = [
            p, re.sub(r"\s+", " ", r["pws_name"]).strip(), r["pws_type_code"], town,
            num(r["population_served_count"]), num(r["service_connections_count"]),
            r["primary_source_code"], r["owner_type_code"], r["is_wholesaler_ind"],
            r["is_school_or_daycare_ind"], r["primacy_agency_code"],
        ]
    sys_text = csv_text(["pwsid", "name", "type", "town", "population", "connections", "source",
                         "owner", "wholesaler", "school_or_child_care", "primacy"],
                        [systems[k] for k in sorted(systems)])

    viol = fetch("violation_ma.csv", f"{EF}/VIOLATION/PRIMACY_AGENCY_CODE/MA/CSV")
    for t in tribal:
        viol += fetch(f"violation_{t['pwsid']}.csv", f"{EF}/VIOLATION/PWSID/{t['pwsid']}/CSV")
    vrows = []
    for r in viol:
        if r["pwsid"] not in systems or r["compl_per_begin_date"][:10] < SINCE:
            continue
        vrows.append([r["pwsid"], r["compl_per_begin_date"][:10], r["compl_per_end_date"][:10],
                      r["violation_code"], r["violation_category_code"], r["is_health_based_ind"],
                      r["contaminant_code"], r["rule_code"], r["compliance_status_code"],
                      r["rtc_date"][:10], num(r["viol_measure"]), r["unit_of_measure"],
                      r["public_notification_tier"]])
    vrows.sort()
    viol_text = csv_text(["pwsid", "begin", "end", "code", "category", "health_based",
                          "contaminant", "rule", "status", "returned_to_compliance", "measure",
                          "unit", "notice_tier"], vrows)

    res = fetch("lcr_sample_result_ma.csv", f"{EF}/LCR_SAMPLE_RESULT/PRIMACY_AGENCY_CODE/MA/CSV")
    smp = fetch("lcr_sample_ma.csv", f"{EF}/LCR_SAMPLE/PRIMACY_AGENCY_CODE/MA/CSV")
    for t in tribal:
        res += fetch(f"lcr_sample_result_{t['pwsid']}.csv",
                     f"{EF}/LCR_SAMPLE_RESULT/PWSID/{t['pwsid']}/CSV")
        smp += fetch(f"lcr_sample_{t['pwsid']}.csv", f"{EF}/LCR_SAMPLE/PWSID/{t['pwsid']}/CSV")
    periods = {(r["pwsid"], r["sample_id"]): r for r in smp}
    lrows = set()
    for r in res:
        s = periods.get((r["pwsid"], r["sample_id"]))
        if s is None or r["pwsid"] not in systems or s["sampling_end_date"][:10] < SINCE:
            continue
        if r["contaminant_code"] not in ("PB90", "CU90") or not num(r["sample_measure"]):
            continue
        if r["unit_of_measure"].lower() != "mg/l":
            raise SystemExit(f"unexpected lead/copper unit {r['unit_of_measure']}")
        lrows.add((r["pwsid"], r["contaminant_code"], s["sampling_start_date"][:10],
                   s["sampling_end_date"][:10], num(r["sample_measure"])))
    lc_text = csv_text(["pwsid", "contaminant", "period_start", "period_end", "value_mg_l"],
                       sorted(lrows))

    codes = fetch("ref_code_value.csv", f"{DMAP}/sdwis.ref_code_value/CSV")
    crow = sorted({(r["value_type"], r["value_code"], re.sub(r"\s+", " ", r["value_description"]).strip())
                   for r in codes if r["value_type"] in CODE_TYPES and r["value_code"]})
    code_text = csv_text(["type", "code", "description"], crow)
    return {
        "ma_water_systems.csv": (sys_text, len(systems)),
        "ma_water_violations.csv": (viol_text, len(vrows)),
        "ma_water_lead_copper.csv": (lc_text, len(lrows)),
        "sdwis_codes.csv": (code_text, len(crow)),
    }, src


# ---------------------------------------------------------------------- PFAS6


def pfas(work, today):
    since = (today - dt.timedelta(days=730)).isoformat()
    items = []
    src = {}
    start = 0
    while True:
        q = urllib.parse.urlencode({"ChemicalName": "PFAS6", "RaworFinished": "F",
                                    "FromCollectedDate": since, "_start": start,
                                    "_end": start + 5000, "ColumnName": "Id", "Direction": "asc"})
        name = f"pfas6_{start}.json"
        raw = get(f"{EEA}?{q}", work / name)
        src[name] = sha(raw)
        d = json.loads(raw)
        items += d["Items"]
        start += 5000
        if start >= d["TotalCount"]:
            break
    if len({i["Id"] for i in items}) != len(items):
        raise SystemExit("PFAS6 pages overlap")
    rows = set()
    for i in items:
        if i["ChemicalName"] != "PFAS6" or i["RaworFinished"] != "F":
            continue
        if (i.get("LowerDetectLimit") or "").upper() != "NG/L":
            raise SystemExit(f"unexpected PFAS6 unit {i.get('LowerDetectLimit')}")
        res = (i["Result"] or "").strip().upper()
        if res != "ND":
            try:
                float(res)
            except ValueError:
                continue
        loc = re.sub(r"\s+", " ", i.get("LocationName") or "").strip()
        rows.add(("MA" + i["PWSId"].strip(), i["CollectedDate"][:10],
                  (i.get("SampleLocCode") or "").strip(), loc, res))
    text = csv_text(["pwsid", "date", "location_code", "location", "result_ng_l"], sorted(rows))
    return text, len(rows), src, since


# --------------------------------------------------------- My Water's Fluoride


def hidden(t):
    f = dict(re.findall(r'<input type="hidden" name="(__[A-Z]+)" id="[^"]*" value="([^"]*)"', t))
    return {k: html.unescape(v) for k, v in f.items()}


def cells(t, table_id):
    i = t.find(f'id="{table_id}"')
    if i < 0:
        raise SystemExit(f"table {table_id} not found")
    tab = t[i:t.find("</table>", i)]
    out = []
    for r in re.findall(r"<tr[^>]*>(.*?)</tr>", tab, flags=re.S):
        out.append([html.unescape(re.sub(r"\s+", " ", re.sub(r"<[^>]+>", "", c))).strip()
                    for c in re.findall(r"<t[hd][^>]*>(.*?)</t[hd]>", r, flags=re.S)])
    return out


def mwf(work):
    src = {}
    rep = f"{MWF}/Reports/Default.aspx"
    t = get(rep).decode("utf-8", "replace")
    f = hidden(t)
    f.update({"__EVENTTARGET": "ctl00$contentArea2$ucStateSelectorSelectState$ddlState",
              "__EVENTARGUMENT": "",
              "ctl00$contentArea2$ucStateSelectorSelectState$ddlState": "25;MA;4",
              "ctl00$contentArea2$selectedStateId": ""})
    get(rep, data=urllib.parse.urlencode(f).encode())

    # Fluoridation Status Report, latest year, with status and water source.
    url = f"{MWF}/Reports/FlStatus_Rpt.aspx"
    t = get(url).decode("utf-8", "replace")
    year = re.search(r'name="ctl00\$contentArea2\$_ddlReportYears".*?<option selected="selected" value="(\d{4})"',
                     t, flags=re.S).group(1)
    f = hidden(t)
    f.update({"__EVENTTARGET": "", "__EVENTARGUMENT": "",
              "ctl00$contentArea2$_ddlReportYears": year,
              "ctl00$contentArea2$_rbtnlstFilter": "1",
              "ctl00$contentArea2$_rbtnlstGroupings": "1",
              "ctl00$contentArea2$_chklstOptionalCols$_chklstOptionalCols_0": "4",
              "ctl00$contentArea2$_chklstOptionalCols$_chklstOptionalCols_1": "9",
              "ctl00$contentArea2$_btnGenerateReport": "View Report"})
    raw = get(url, work / "mwf_flstatus.html", data=urllib.parse.urlencode(f).encode())
    src["mwf_flstatus.html"] = sha(raw)
    status = cells(raw.decode("utf-8", "replace"), "_gvCensusRpt")
    if status[0] != ["PWS ID", "PWS Name", "County", "Population Served", "Fluoridated", "Status",
                     "Water Source"]:
        raise SystemExit(f"unexpected status report header {status[0]}")

    # Average fluoride levels by month (adjusted systems), same year.
    url = f"{MWF}/Reports/AvgFlLevel_Month_Rpt.aspx"
    raw = get(url, work / "mwf_avgfl.html")
    src["mwf_avgfl.html"] = sha(raw)
    t = raw.decode("utf-8", "replace")
    avg_year = re.search(r"Average Fluoride Levels by Month - Massachusetts, (\d{4})", t).group(1)
    avg = {}
    for r in cells(t, "Grd_AverageFLLevel")[1:]:
        if len(r) >= 15 and r[0].startswith("MA"):
            avg[r[0].replace(" ", "").replace("-", "")] = r[14]

    # Monthly summary (latest month).
    url = f"{MWF}/Reports/Summary_Rpt.aspx"
    raw = get(url, work / "mwf_summary.html")
    src["mwf_summary.html"] = sha(raw)
    t = raw.decode("utf-8", "replace")
    period = re.search(r"Monthly Summary Report \(Massachusetts - ([A-Za-z]+, \d{4})\)", t).group(1)
    text = html.unescape(re.sub(r"\s+", " ", re.sub(r"<[^>]+>", " ", t)))
    i = text.find("Fluoridation Status for")
    j = text.find("Operational Summary", i)
    block = text[i:j]
    summary = []
    m = re.search(r"All Water Systems ([\d,]+) ([\d,]+)", block)
    summary.append(["All community water systems", "Total", m.group(1).replace(",", ""),
                    m.group(2).replace(",", "")])
    for group, body in (("Fluoridated", block[block.find("Fluoridated Adjusted"):block.find("Non-Fluoridated")]),
                        ("Non-fluoridated", block[block.find("Non-Fluoridated"):])):
        for kind in ("Adjusted", "Natural", "Non-Adjusted", "Variable/Other", "Defluoridated",
                     "Consecutive", "Multi-source", "Total"):
            mm = re.search(r"(?<![-\w])" + re.escape(kind) + r" ([\d,]+) ([\d,]+)", body)
            if mm:
                summary.append([group, kind, mm.group(1).replace(",", ""), mm.group(2).replace(",", "")])
    return status[1:], year, avg, avg_year, summary, period, src


def match_mwf(status, sdwis_rows, towns):
    """Assign each MWF record a SDWIS PWSID (when one matches) and a town."""
    sd = {r[0]: r for r in sdwis_rows}
    by6 = {}
    for p in sd:
        by6.setdefault(p[3:], []).append(p)

    def similar(a, b):
        ta, tb = set(norm(a).split()), set(norm(b).split())
        stop = {"water", "department", "dept", "district", "the", "of", "and", "dpw", "division",
                "inc", "co", "company", "wtr", "mwra", "town", "works", "system", "supply"}
        ta, tb = ta - stop, tb - stop
        return bool(ta & tb) or (not ta and not tb)

    out = []
    for pid, name, county, pop, fl, st, srcw in status:
        p = pid.replace("-", "").replace(" ", "")
        sdw, method, town = "", "", ""
        # "PELHAM (PART)", "ROYALSTON (SRIC) PART": the part of a town served by
        # another town's system. Matched to SDWIS only by an identical PWSID.
        part = re.search(r"\bpart\b", name, flags=re.I)
        if part:
            town = town_in_name(name, towns) or ""
            method = "named part of a town"
            if p in sd:
                sdw = p
        elif p in sd and similar(name, sd[p][1]):
            sdw, method = p, method or "same PWSID"
        else:
            cands = [q for q in by6.get(p[3:], []) if similar(name, sd[q][1])]
            if len(cands) == 1:
                sdw, method = cands[0], method or "PWSID region digit differs"
            else:
                same = [q for q in sd if norm(sd[q][1]) == norm(name)]
                if len(same) == 1:
                    sdw, method = same[0], method or "same system name"
                elif p in sd:
                    sdw, method = p, method or "same PWSID (names differ)"
        if not town:
            town = (sd[sdw][3] if sdw else "") or town_in_name(name, towns) or pws_town(p, towns) or ""
            if not method:
                method = "town in name" if town_in_name(name, towns) else ("PWSID town number" if town else "none")
        out.append([pid, name, county.replace(" (Primary)", ""), pop.replace(",", ""), fl, st, srcw,
                    town, sdw, method])
    # Two MWF records for one SDWIS system: label the one whose PWSID differs
    # as a duplicate (the tool shows both and flags disagreement).
    seen = {}
    for r in out:
        if r[8]:
            seen.setdefault(r[8], []).append(r)
    for k, rs in seen.items():
        if len(rs) > 1:
            exact = [r for r in rs if r[0].replace("-", "") == k]
            for r in rs:
                if exact and r is not exact[0]:
                    r[9] = f"duplicate of {exact[0][0]}"
    return out


# ------------------------------------------------------------------------ DPH


def dph(work):
    raw = get(DPH_CSV, work / "dph_fluoridation_2023.csv")
    rows = list(csv.reader(io.StringIO(raw.decode("utf-8-sig"))))
    out = []
    for r in rows[1:]:
        if not r or not r[0].strip():
            continue
        town = r[0].replace("\xa0", " ").strip()
        mwra = town.endswith("*")
        out.append([town.rstrip("*").strip(), r[1].strip(), r[2].replace(",", "").strip(),
                    r[3].strip(), "Y" if mwra else "N", re.sub(r"\s+", " ", r[4]).strip()])
    return csv_text(["town", "status", "population", "year_began", "mwra", "information"], out), \
        len(out), {"dph_fluoridation_2023.csv": sha(raw)}


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    work = Path(sys.argv[1])
    work.mkdir(parents=True, exist_ok=True)
    check = "--check" in sys.argv
    today = dt.date.today()
    towns = load_towns()
    results = {}
    files, src = sdwis(work, towns)
    for k, (text, n) in files.items():
        results[k] = (text, n, src, "")
    text, n, psrc, since = pfas(work, today)
    results["ma_water_pfas6.csv"] = (text, n, psrc, f"finished-water PFAS6 results collected {since} to {today}")
    status, year, avg, avg_year, summary, period, msrc = mwf(work)
    sys_rows = list(csv.reader(io.StringIO(files["ma_water_systems.csv"][0])))[1:]
    matched = match_mwf(status, sys_rows, towns)
    for r in matched:
        r.append(avg.get(r[0].replace("-", ""), "") if avg_year == year else "")
        r.append(year)
    results["ma_fluoridation_systems.csv"] = (
        csv_text(["mwf_pwsid", "name", "county", "population", "fluoridated", "status",
                  "water_source", "town", "sdwis_pwsid", "match", "avg_fluoride_mg_l",
                  "report_year"], matched), len(matched), msrc,
        f"Fluoridation Status Report {year}; average levels {avg_year}")
    results["ma_fluoridation_summary.csv"] = (
        csv_text(["group", "status", "systems", "population"],
                 [[g, s, a, b] for g, s, a, b in summary]), len(summary), msrc,
        f"Monthly Summary Report, {period}")
    text, n, dsrc = dph(work)
    results["ma_fluoridation_towns.csv"] = (text, n, dsrc, "DPH 2023 list")
    for name, (text, rows, _src, note) in results.items():
        data = text.encode("utf-8")
        path = SNAP / name
        changed = not path.exists() or path.read_bytes() != data
        if not check:
            path.write_bytes(data)
        print(json.dumps({"path": name, "sha256": sha(data), "bytes": len(data), "rows": rows,
                          "note": note, "changed": changed}))
    print(json.dumps({"source_sha256": {**src, **psrc, **msrc, **dsrc}}, indent=1))


if __name__ == "__main__":
    main()
