"""Build the FindTreatment.gov snapshot of Massachusetts substance use treatment facilities.

usage: build_findtreatment.py OUT_CSV

Downloads (with curl) every substance use facility in Massachusetts from SAMHSA's FindTreatment.gov
locator API (state search: limitType=0, limitValue=3, sType=sa, pageSize=2000) and keeps the
facility name, address, phones, website, coordinates, and these service categories (as
"CODE=value;value" pairs joined by "|"): TC type of care, SET service setting, OT type of opioid
treatment, OM opioid medications, PAY payment accepted, PYAS payment assistance, SL language
services, OL other languages, AGE age groups, SN sex accepted, RSS recovery support services,
SG special programs, EXCL exclusive services, DETOX detoxification services.
"""
import csv, json, subprocess, sys

URL = ("https://findtreatment.gov/locator/exportsAsJson/v2?sAddr=%2242.3601,-71.0589%22"
       "&limitType=0&limitValue=3&sType=sa&pageSize=2000&page=1&sort=0")
KEEP = ["TC", "SET", "OT", "OM", "PAY", "PYAS", "SL", "OL", "AGE", "SN", "RSS", "SG", "EXCL", "DETOX"]
data = json.loads(subprocess.run(["curl", "-sS", "--fail", "--max-time", "180", URL],
                                 check=True, capture_output=True).stdout)
rows = data["rows"]
assert data["recordCount"] == len(rows), (data["recordCount"], len(rows))
fields = ["name1", "name2", "street1", "street2", "city", "state", "zip", "phone", "intake1",
          "hotline1", "website", "latitude", "longitude"]
with open(sys.argv[1], "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f, lineterminator="\n")
    w.writerow(fields + ["services"])
    for r in sorted(rows, key=lambda r: (r.get("city") or "", r.get("name1") or "", r.get("street1") or "")):
        svc = [s["f2"] + "=" + s["f3"] for s in r.get("services") or [] if s.get("f2") in KEEP]
        w.writerow([(r.get(k) or "") for k in fields] + ["|".join(svc)])
print("facilities", len(rows))
