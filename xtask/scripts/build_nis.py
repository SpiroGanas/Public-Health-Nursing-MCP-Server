"""Build snapshots/nis_coverage_ma_us.csv from data.cdc.gov (NIS-Child, NIS-Teen,
FluVaxView, SchoolVaxView). Writes CSV to argv[1]; prints the query URLs used."""
import csv, json, sys, urllib.parse, urllib.request

OUT = sys.argv[1]
GEO = "geography in('Massachusetts','United States')"
QUERIES = [
    ("nis_child", "fhky-rtsk",
     "vaccine,dose,geography,year_season,dimension,coverage_estimate,_95_ci,population_sample_size",
     GEO + " AND dimension_type='Age' AND dimension in('24 Months','35 Months') AND year_season in('2018','2019','2020','2021','2022')"),
    ("nis_teen", "ee48-w5t6",
     "vaccine,dose,geography,year_season,dimension,coverage_estimate,_95_ci,population_sample_size",
     GEO + " AND dimension_type='Age' AND dimension='13-17 Years' AND year_season in('2020','2021','2022','2023','2024','2025')"),
    ("flu", "vh55-3he6",
     "vaccine,geography,year_season,month,dimension,coverage_estimate,_95_ci,population_sample_size",
     GEO + " AND dimension_type='Age' AND month='5' AND year_season in('2021-22','2022-23','2023-24','2024-25','2025-26')"),
    ("kindergarten", "ijqb-a7ye",
     "vaccine,dose,geography,year_season,coverage_estimate,population_sample_size,percent_surveyed,number_of_exemptions",
     GEO + " AND year_season in('2021-22','2022-23','2023-24','2024-25','2025-26')"),
]

rows = []
for survey, ds, select, where in QUERIES:
    q = urllib.parse.urlencode({"$select": select, "$where": where, "$limit": "5000"})
    url = f"https://data.cdc.gov/resource/{ds}.json?{q}"
    print(url)
    with urllib.request.urlopen(url, timeout=90) as r:
        data = json.load(r)
    for d in data:
        rows.append({
            "survey": survey,
            "dataset": ds,
            "vaccine": d.get("vaccine", ""),
            "dose": d.get("dose", ""),
            "geography": d.get("geography", ""),
            "period": d.get("year_season", ""),
            "group": d.get("dimension", "Kindergarten" if survey == "kindergarten" else ""),
            "coverage_pct": d.get("coverage_estimate", ""),
            "ci_95": d.get("_95_ci", ""),
            "sample_size": d.get("population_sample_size", ""),
            "exemptions": d.get("number_of_exemptions", ""),
        })
rows.sort(key=lambda r: (r["survey"], r["vaccine"], r["dose"], r["group"], r["period"], r["geography"]))
with open(OUT, "w", newline="", encoding="utf-8") as f:
    w = csv.DictWriter(f, fieldnames=list(rows[0].keys()), lineterminator="\n")
    w.writeheader()
    w.writerows(rows)
print(len(rows), "rows")
