#!/usr/bin/env python3
"""Generates the Phase 18 workforce synthetic files and the expected values the Rust tests compare
with. The expected values are computed here, independently of the Rust code (plain Python
arithmetic and calendar-month addition), from the generated CSVs:

    python3 crates/phn-server/tests/fixtures/phase18_workforce/generate.py

Writes fixtures/synthetic/data/workforce_ratings.csv and workforce_credentials.csv and
expected_workforce.json (next to this script). Everything is fictional.
"""
import calendar
import csv
import datetime as dt
import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent
DATA = HERE.parents[4] / "fixtures" / "synthetic" / "data"
DOMAINS = ["Data Analytics and Assessment Skills", "Policy Development and Program Planning Skills",
           "Communication Skills", "Health Equity Skills", "Community Partnership Skills",
           "Public Health Sciences Skills", "Management and Finance Skills",
           "Leadership and Systems Thinking Skills"]
SUP = {"S": ("Octavian Pembleton-Rhys", "octavian.pembletonrhys@example.org", "413-555-0193")}

# staff -> tier, and the rating in each of the 8 domains (None = not rated)
RATINGS = {
    "S01": (1, [2, 3, 3, 2, 3, 4, 2, 1]),
    "S02": (1, [3, 3, 2, 2, 4, 3, 2, 2]),
    "S03": (1, [3, 4, 3, 3, 3, 3, 1, None]),
    "S04": (1, [2, 3, 4, 3, 3, 2, None, None]),
    "S05": (2, [4, 3, 3, 4, 4, 3, 3, None]),
    "S06": (2, [3, 3, None, 3, None, 4, 3, None]),
}

CRED = [
    # code, position, hire, [(credential, completed)]
    ("N01", "Public Health Nurse", "2026-01-15", [("MAVEN training", "2026-06-01")]),
    ("N02", "Public Health Nurse", "2025-09-01", [("MAVEN training", "2025-12-01"), ("Foundations course", "2026-05-01")]),
    ("N03", "Public Health Nurse", "2026-06-01", [("MAVEN training", "2026-09-01")]),
    ("N04", "Public Health Nurse", "2024-02-01", [("MAVEN training", "2024-05-01")]),
    ("N05", "Public Health Nurse", "2023-03-01", [("MAVEN training", "2023-06-01"), ("Foundations course", "2023-12-01"), ("ICS 100 and NIMS 700", "2023-04-01")]),
    ("N06", "Public Health Nurse", "2022-05-01", [("MAVEN training", "2022-07-01"), ("Foundations course", "2022-11-01"), ("ICS 100 and NIMS 700", "2022-06-01")]),
    ("N07", "Public Health Nurse", "2025-01-06", [("MAVEN training", "2025-03-01"), ("Foundations course", "2025-10-01"), ("ICS 100 and NIMS 700", "2025-02-01")]),
    ("I01", "Inspector", "2025-03-10", [("Foundations course", "2025-08-01")]),
    ("V01", "Volunteer driver", "2025-05-01", []),
]
AS_OF = dt.date(2026, 11, 12)
SOON = 90
TARGET = 3.0


def add_months(d, m):
    y = d.year + (d.month - 1 + m) // 12
    mo = (d.month - 1 + m) % 12 + 1
    return dt.date(y, mo, min(d.day, calendar.monthrange(y, mo)[1]))


def write_csvs():
    name, email, phone = SUP["S"]
    with open(DATA / "workforce_ratings.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["Staff Code", "Tier", "Domain", "Competency", "Rating", "Supervisor Name", "Supervisor Email"])
        for code, (tier, rs) in RATINGS.items():
            for i, r in enumerate(rs):
                w.writerow([code, tier, i + 1, "", "" if r is None else r, name, email])
    with open(DATA / "workforce_credentials.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["Staff Code", "Position", "Hire Date", "Credential", "Completed Date", "Supervisor Name", "Supervisor Phone"])
        for code, pos, hire, done in CRED:
            if not done:
                w.writerow([code, pos, hire, "", "", name, phone])
            for c, d in done:
                w.writerow([code, pos, hire, c, d, name, phone])


def mean(v):
    return sum(v) / len(v)


def expected():
    out = {"domains": [], "tiers": {}, "target": TARGET}
    staff = set(RATINGS)
    out["team_size"] = len(staff)
    shown = []
    for i, name in enumerate(DOMAINS):
        vals = [rs[i] for _, rs in RATINGS.values() if rs[i] is not None]
        if len(vals) < 3:
            out["domains"].append({"domain": name, "raters": None, "mean": None})
            continue
        m = round(mean(vals), 2)
        below = sum(1 for v in vals if v < TARGET)
        out["domains"].append({"domain": name, "raters": len(vals), "mean": m, "gap": round(TARGET - mean(vals), 2),
                               "below": below if (below == 0 or below >= 3) else None})
        shown.append(mean(vals))
    out["overall"] = round(mean(shown), 2)
    for t in (1, 2, 3):
        members = [c for c, (tier, _) in RATINGS.items() if tier == t]
        if len(members) >= 3:
            vals = [r for c in members for r in RATINGS[c][1] if r is not None]
            out["tiers"][str(t)] = {"staff": len(members), "mean": round(mean(vals), 2)}
        elif members:
            out["tiers"][str(t)] = None
    # Credentials (public health nurse standard: MAVEN 6 months, Foundations 12, ICS 100/NIMS 700 12).
    reqs = {"MAVEN-trained": 6, "Foundations course": 12, "ICS 100 and NIMS 700": 12}
    keys = {"MAVEN-trained": "MAVEN training", "Foundations course": "Foundations course", "ICS 100 and NIMS 700": "ICS 100 and NIMS 700"}
    statuses = {}
    for code, pos, hire, done in CRED:
        if pos != "Public Health Nurse":
            continue
        h = dt.date.fromisoformat(hire)
        d = {c: dt.date.fromisoformat(x) for c, x in done}
        for label, months in reqs.items():
            due = add_months(h, months)
            c = d.get(keys[label])
            if c is not None:
                s = "complete (late)" if c > due else "complete"
            elif AS_OF > due:
                s = "overdue"
            elif (due - AS_OF).days <= SOON:
                s = "due soon"
            else:
                s = "upcoming"
            statuses.setdefault(label, {})[code] = s
    summ = {}
    for label, by in statuses.items():
        v = list(by.values())
        complete = sum(1 for s in v if s.startswith("complete"))
        attention = sum(1 for s in v if s in ("overdue", "due soon"))
        upcoming = sum(1 for s in v if s == "upcoming")
        hide = len(v) < 3 or any(0 < n < 3 for n in (complete, attention, upcoming))
        summ[label] = None if hide else {"staff": len(v), "complete": complete, "attention": attention, "upcoming": upcoming}
    out["credential_statuses"] = statuses
    out["credential_summary"] = summ
    return out


if __name__ == "__main__":
    write_csvs()
    (HERE / "expected_workforce.json").write_text(json.dumps(expected(), indent=2) + "\n")
    print("wrote fixtures and expected_workforce.json")
