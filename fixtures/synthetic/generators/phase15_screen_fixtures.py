#!/usr/bin/env python3
"""Write the Phase 15 screening fixtures (CHR-02, CHR-03) and register their identifiers.

Usage (from the repository root): python3 -I fixtures/synthetic/generators/phase15_screen_fixtures.py

Writes fixtures/synthetic/data/bp_screening_log.csv (a fictional community blood pressure
screening log with names, addresses, and phone numbers that must never be returned) and
fixtures/synthetic/data/prediabetes_responses.csv (fictional health fair risk test answers with
names and phone numbers), then adds their seeded identifiers to
fixtures/synthetic/seeded_identifiers.json by (file, kind, value). All values are fictional
(555 phone numbers, invented surnames). Fixed seed: reruns write the same files.
"""
import csv
import json
import random
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
DATA = ROOT / "fixtures/synthetic/data"
SEEDED = ROOT / "fixtures/synthetic/seeded_identifiers.json"

FIRST = ["Aldous", "Berenice", "Caspar", "Dorothea", "Evelina", "Fitzroy", "Georgiana", "Hadrian",
         "Isolde", "Jocasta", "Kendrick", "Lavinia", "Maximilian", "Nerida", "Orlando", "Petronella",
         "Quentin", "Rosalie", "Sylvester", "Theodora", "Ulric", "Verity", "Winslow", "Yolande"]
LAST = ["Applewhaite", "Brindlemoor", "Candlewick", "Dunmarrow", "Elderthorn", "Fallowmere",
        "Greywether", "Hollinbrook", "Ironsedge", "Juniperley", "Kettlewell", "Lowenthorpe",
        "Mistlethwaite", "Nettlebed", "Oakhaven", "Pennywhistle", "Quillsworth", "Ravenscar",
        "Stonebrook", "Thistledown", "Underwood-Pyke", "Valeroot", "Wexmoor", "Yarrowby"]
STREETS = ["Sablewood Lane", "Cinderfield Road", "Marrowgate Street", "Tansy Court", "Wicklow Way",
           "Heronsfoot Avenue"]
TOWNS = ["Holyoke", "Chicopee", "Springfield", "Westfield"]
SITES = ["Maple Grove Senior Center", "Riverside Health Fair"]


def main():
    rnd = random.Random(1501)
    seeded = []

    def seed(file, kind, value, column):
        seeded.append({"value": value, "kind": kind, "file": f"data/{file}", "column": column})

    # ---- blood pressure screening log: 30 rows
    rows = []
    for i in range(30):
        first, last = FIRST[i % 24], LAST[(i * 5 + 3) % 24]
        age = rnd.choice([34, 41, 47, 52, 58, 63, 67, 71, 76, 83])
        band = rnd.random()
        if band < 0.35:
            s1, d1 = rnd.randint(108, 119), rnd.randint(66, 78)
        elif band < 0.50:
            s1, d1 = rnd.randint(120, 129), rnd.randint(68, 78)
        elif band < 0.70:
            s1, d1 = rnd.randint(130, 139), rnd.randint(78, 88)
        elif band < 0.93:
            s1, d1 = rnd.randint(140, 168), rnd.randint(88, 104)
        else:
            s1, d1 = rnd.randint(182, 196), rnd.randint(104, 124)
        two = rnd.random() < 0.8
        s2, d2 = (s1 + rnd.randint(-4, 4), d1 + rnd.randint(-3, 3)) if two else ("", "")
        ref = ("Not referred" if s1 < 130 else "Referred routine" if s1 < 181 and d1 < 121
               else "Referred same day")
        addr = f"{100 + i * 7} {STREETS[i % 6]}"
        phone = f"413-555-{300 + i:04d}"
        rows.append([f"BP{i + 1:03d}", f"2026-10-{10 + i % 12:02d}", SITES[i % 2], age,
                     f"{first} {last}", addr, TOWNS[i % 4], phone, s1, d1, s2, d2, ref])
        seed("bp_screening_log.csv", "name", first, "Name")
        seed("bp_screening_log.csv", "name", last, "Name")
        seed("bp_screening_log.csv", "street_address", addr, "Address")
        seed("bp_screening_log.csv", "phone", phone, "Phone")
    # One participant under 18 and one with no usable reading.
    rows.append(["BP031", "2026-10-21", SITES[0], 15, "Cosmo Briarwood", "9 Wren Street", "Holyoke",
                 "413-555-0399", 104, 66, "", "", "Not referred"])
    rows.append(["BP032", "2026-10-21", SITES[1], 60, "Dulcie Everhart", "11 Wren Street", "Holyoke",
                 "413-555-0398", "", "", "", "", "Not referred"])
    for nm, addr, ph in [("Cosmo Briarwood", "9 Wren Street", "413-555-0399"),
                         ("Dulcie Everhart", "11 Wren Street", "413-555-0398")]:
        f, l = nm.split()
        seed("bp_screening_log.csv", "name", f, "Name")
        seed("bp_screening_log.csv", "name", l, "Name")
        seed("bp_screening_log.csv", "street_address", addr, "Address")
        seed("bp_screening_log.csv", "phone", ph, "Phone")
    with open(DATA / "bp_screening_log.csv", "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f, lineterminator="\n")
        w.writerow(["Participant ID", "Screening Date", "Site", "Age", "Name", "Address", "Town",
                    "Phone", "Systolic 1", "Diastolic 1", "Systolic 2", "Diastolic 2",
                    "Referral Status"])
        w.writerows(rows)

    # ---- prediabetes risk test responses: 26 rows
    rows = []
    for i in range(26):
        first, last = FIRST[(i * 7 + 2) % 24], LAST[(i * 3 + 1) % 24]
        age = rnd.choice([29, 38, 44, 49, 53, 57, 61, 66, 72])
        sex = rnd.choice(["woman", "man"])
        fam = rnd.choice(["yes", "no"])
        hbp = rnd.choice(["yes", "no", "no"])
        act = rnd.choice(["yes", "yes", "no"])
        height = rnd.choice([62, 64, 66, 68, 70, 72])
        weight = rnd.choice([125, 150, 175, 200, 230, 260, 290])
        phone = f"413-555-{400 + i:04d}"
        rows.append([f"{first} {last}", age, sex, fam, hbp, act, height, weight, phone])
        seed("prediabetes_responses.csv", "name", first, "Name")
        seed("prediabetes_responses.csv", "name", last, "Name")
        seed("prediabetes_responses.csv", "phone", phone, "Phone")
    with open(DATA / "prediabetes_responses.csv", "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f, lineterminator="\n")
        w.writerow(["Name", "Age", "Sex", "Family history", "High blood pressure",
                    "Physically active", "Height", "Weight", "Phone"])
        w.writerows(rows)

    d = json.loads(SEEDED.read_text(encoding="utf-8"))
    have = {(i["file"], i["kind"], i["value"]) for i in d["identifiers"]}
    for s in seeded:
        k = (s["file"], s["kind"], s["value"])
        if k not in have:
            d["identifiers"].append(s)
            have.add(k)
    SEEDED.write_text(json.dumps(d, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
