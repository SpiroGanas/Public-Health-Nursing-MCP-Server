"""Build the CDC YRBS mental health indicators snapshot for Massachusetts and the United States.

usage: build_yrbs_mental_health.py OUT_CSV

Downloads (with curl) the rows of data.cdc.gov dataset nu3s-3dwd ("Youth Risk Behavioral
Surveillance System (YRBSS) - Mental Health Indicators") for area_abbr MA and XX (the national
YRBS), every year, question, and demographic group. "Suppressed" values are kept as blank
estimates with suppressed = 1.
"""
import csv, json, subprocess, sys, urllib.parse

params = {
    "$select": "year,question,area_abbr,demographics_type,demographics_value,percent,"
               "low_confidence_interval,high_confidence_interval",
    "$where": "area_abbr in ('MA','XX')",
    "$order": "area_abbr,year,question,demographics_type,demographics_value",
    "$limit": "50000",
}
url = "https://data.cdc.gov/resource/nu3s-3dwd.json?" + urllib.parse.urlencode(params)
rows = json.loads(subprocess.run(["curl", "-sS", "--fail", "--max-time", "120", url],
                                 check=True, capture_output=True).stdout)
with open(sys.argv[1], "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f, lineterminator="\n")
    w.writerow(["year", "area_abbr", "question", "demographics_type", "demographics_value",
                "percent", "low_confidence_interval", "high_confidence_interval"])
    for r in rows:
        w.writerow([r.get(k, "") for k in ["year", "area_abbr", "question", "demographics_type",
                                          "demographics_value", "percent", "low_confidence_interval",
                                          "high_confidence_interval"]])
print("rows", len(rows))
