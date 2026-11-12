#!/usr/bin/env python3
"""Write the Phase 16 quality improvement fixture (EBP-05) and register its identifiers.

Usage (from the repository root): python3 -I fixtures/synthetic/generators/phase16_qi_fixture.py

Writes fixtures/synthetic/data/qi_clinic_measure.csv: a fictional clinic's monthly measure log
(children seen, children up to date on recommended vaccines) with one row per month, 20 months
from 2025-01, a step up in the share up to date from October 2025 (a documented change), one
month with a small count, and the fictional data-entry contact person names, phone numbers, and emails
that must never be returned. Then adds those identifiers to
fixtures/synthetic/seeded_identifiers.json by (file, kind, value). All values are fictional
(555 phone numbers, invented surnames). Fixed seed: reruns write the same file.
"""
import csv
import json
import random
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
DATA = ROOT / "fixtures/synthetic/data"
SEEDED = ROOT / "fixtures/synthetic/seeded_identifiers.json"

STAFF = [
    ("Odalys Fennimore-Vance", "413-555-0188", "odalys.fennimorevance@example.org"),
    ("Torvald Pemberwick", "413-555-0189", "torvald.pemberwick@example.org"),
]


def main():
    rnd = random.Random(1601)
    rows = []
    for m in range(20):
        year, month = 2025 + m // 12, m % 12 + 1
        seen = rnd.randint(38, 62)
        p = 0.66 + rnd.uniform(-0.04, 0.04) if m < 9 else 0.80 + rnd.uniform(-0.04, 0.04)
        up = round(seen * p)
        note = ""
        if m == 9:
            note = "Reminder letters started this month"
        if m == 15:
            # A small-count month (below the suppression threshold of 5 for the not-up-to-date count).
            seen, up, note = 6, 3, "Clinic closed two weeks for renovation"
        staff = STAFF[m % 2]
        rows.append([f"{year}-{month:02d}", seen, up, staff[0], staff[1], staff[2], note])
    with open(DATA / "qi_clinic_measure.csv", "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["Month", "Children Seen", "Children Up To Date", "Contact Person", "Contact Cell", "Contact Email", "Notes"])
        w.writerows(rows)

    seeded = []
    for name, phone, email in STAFF:
        seeded.append({"value": name, "kind": "name", "file": "data/qi_clinic_measure.csv", "column": "Contact Person"})
        seeded.append({"value": phone, "kind": "phone", "file": "data/qi_clinic_measure.csv", "column": "Contact Cell"})
        seeded.append({"value": email, "kind": "email", "file": "data/qi_clinic_measure.csv", "column": "Contact Email"})
    d = json.loads(SEEDED.read_text(encoding="utf-8"))
    have = {(i["file"], i["kind"], i["value"]) for i in d["identifiers"]}
    for s in seeded:
        if (s["file"], s["kind"], s["value"]) not in have:
            d["identifiers"].append(s)
    SEEDED.write_text(json.dumps(d, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
