"""Build the CDC provisional drug overdose death snapshots for Massachusetts.

usage: build_overdose_vsrr.py OUT_DIR

Downloads (with curl) from data.cdc.gov:
* VSRR Provisional Drug Overdose Death Counts (xkb8-kh2a), Massachusetts and the United States,
  12 month-ending periods from January 2019, nine indicators -> cdc_vsrr_overdose_ma_us.csv
* VSRR Provisional County-Level Drug Overdose Death Counts (gb4e-yj24), the 14 Massachusetts
  counties, from January 2020 -> cdc_vsrr_overdose_ma_counties.csv
Counts of 1-9 are suppressed by NCHS in the county file (blank count, footnote kept).
"""
import csv, json, os, subprocess, sys, urllib.parse

OUT = sys.argv[1]
INDICATORS = [
    "Number of Drug Overdose Deaths",
    "Opioids (T40.0-T40.4,T40.6)",
    "Synthetic opioids, excl. methadone (T40.4)",
    "Heroin (T40.1)",
    "Natural & semi-synthetic opioids (T40.2)",
    "Methadone (T40.3)",
    "Cocaine (T40.5)",
    "Psychostimulants with abuse potential (T43.6)",
    "Percent with drugs specified",
]
MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August",
          "September", "October", "November", "December"]


def soda(dataset, params):
    url = f"https://data.cdc.gov/resource/{dataset}.json?" + urllib.parse.urlencode(params)
    body = subprocess.run(["curl", "-sS", "--fail", "--max-time", "120", url],
                          check=True, capture_output=True).stdout
    return json.loads(body)


ind = ",".join("'" + i.replace("'", "''") + "'" for i in INDICATORS)
rows = soda("xkb8-kh2a", {
    "$select": "state,year,month,indicator,data_value,predicted_value,percent_complete,"
               "percent_pending_investigation,footnote_symbol",
    "$where": f"state in ('MA','US') AND year >= '2019' AND indicator in ({ind})",
    "$limit": "50000",
})
rows.sort(key=lambda r: (r["state"], int(r["year"]), MONTHS.index(r["month"]), INDICATORS.index(r["indicator"])))
fields = ["state", "year", "month", "indicator", "data_value", "predicted_value", "percent_complete",
          "percent_pending_investigation", "footnote_symbol"]
with open(os.path.join(OUT, "cdc_vsrr_overdose_ma_us.csv"), "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f, lineterminator="\n")
    w.writerow(fields)
    for r in rows:
        w.writerow([r.get(k, "") for k in fields])
print("state rows", len(rows), "latest", rows[-1]["year"], rows[-1]["month"])

crows = soda("gb4e-yj24", {
    "$select": "data_as_of,year,month,countyname,fips,code2013,provisional_drug_overdose,footnote,"
               "percentage_of_records_pending",
    "$where": "st_abbrev = 'MA' AND year >= '2020'",
    "$limit": "50000",
})
crows.sort(key=lambda r: (r["fips"], int(r["year"]), int(r["month"])))
cfields = ["year", "month", "countyname", "fips", "code2013", "provisional_drug_overdose", "footnote",
           "percentage_of_records_pending", "data_as_of"]
with open(os.path.join(OUT, "cdc_vsrr_overdose_ma_counties.csv"), "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f, lineterminator="\n")
    w.writerow(cfields)
    for r in crows:
        v = dict(r)
        v["data_as_of"] = v.get("data_as_of", "")[:10]
        w.writerow([v.get(k, "") for k in cfields])
print("county rows", len(crows), "latest", crows[-1]["year"], crows[-1]["month"], crows[-1].get("data_as_of"))
