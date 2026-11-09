"""Build the Massachusetts HRSA health center site snapshot.

usage: build_hrsa_health_centers.py SRC_CSV OUT_CSV

SRC_CSV is HRSA's "Health Center Service Delivery and Look-Alike Sites" download
(https://data.hrsa.gov/DataDownload/DD_Files/Health_Center_Service_Delivery_and_LookAlike_Sites.csv).
Keeps active Massachusetts sites that deliver services (drops administrative-only sites) and the
columns the server uses. Python 3 standard library only.
"""
import csv, sys

SRC, OUT = sys.argv[1:3]
COLS = [
    ("site_name", "Site Name"),
    ("health_center", "Health Center Name"),
    ("center_type", "Health Center Type"),
    ("address", "Site Address"),
    ("city", "Site City"),
    ("zip", "Site Postal Code"),
    ("phone", "Site Telephone Number"),
    ("website", "Site Web Address"),
    ("hours_per_week", "Operating Hours per Week"),
    ("setting", "Health Center Service Delivery Site Location Setting Description"),
    ("location_type", "Health Center Location Type Description"),
    ("site_type", "Health Center Type Description"),
    ("schedule", "Health Center Operational Schedule Description"),
    ("calendar", "Health Center Operating Calendar"),
    ("county", "Complete County Name"),
    ("longitude", "Geocoding Artifact Address Primary X Coordinate"),
    ("latitude", "Geocoding Artifact Address Primary Y Coordinate"),
]
rows = []
with open(SRC, encoding="utf-8-sig", newline="") as f:
    for r in csv.DictReader(f):
        if r.get("Site State Abbreviation") != "MA" or r.get("Site Status Description") != "Active":
            continue
        if r.get("Health Center Type Description", "").strip() == "Administrative":
            continue
        out = {k: (r.get(src) or "").strip() for k, src in COLS}
        out["city"] = out["city"].title() if out["city"].isupper() else out["city"]
        out["health_center"] = out["health_center"].title() if out["health_center"].isupper() else out["health_center"]
        out["county"] = out["county"].replace(" County", "")
        rows.append(out)
rows.sort(key=lambda r: (r["city"], r["site_name"]))
with open(OUT, "w", newline="", encoding="utf-8") as f:
    w = csv.DictWriter(f, fieldnames=[k for k, _ in COLS], lineterminator="\n")
    w.writeheader()
    w.writerows(rows)
print(OUT, len(rows))
