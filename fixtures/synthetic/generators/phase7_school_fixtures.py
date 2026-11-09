"""Generate the fictional Phase 7 school health fixtures (standard library only).

Writes, into fixtures/synthetic/data/:
  school_screenings.csv     screening roster with results (track_school_screenings, annual report)
  school_absence_log.csv    one row per absent student-day (monitor_school_absenteeism)
  health_office_visits.csv  health office visit log (build_school_health_annual_report)
  concussion_log.csv        head injury log (concussion_return_to_learn_tracker)
  health_plan_students.csv  students needing health plans (build_individualized_health_plan)
and prints the seeded identifiers as JSON for fixtures/synthetic/seeded_identifiers.json.

Every value is fictional: phone numbers use 555-01xx, emails example.org, IDs start with 95.
Run from the repository root:  python3 fixtures/synthetic/generators/phase7_school_fixtures.py
"""
import csv
import datetime as dt
import json
import random
import sys
from pathlib import Path

random.seed(20261108)
OUT = Path("fixtures/synthetic/data")

FIRST = ["Amaru", "Bexley", "Corisande", "Dariush", "Elowen", "Faustine", "Gethin", "Ilsabet",
         "Jorvik", "Lysander", "Marisol", "Nikolai", "Oriel", "Peregrin", "Quintessa", "Rhosyn",
         "Seraphine", "Tadhg", "Ulyana", "Valdemar", "Wenceslas", "Xiomara", "Yevgenia", "Zephyrine",
         "Anselm", "Brisa", "Cosmin", "Dagny", "Eirlys", "Florentyna", "Gwilym", "Hesper"]
LAST = ["Brennecke", "Castellucci", "Dragomir", "Esterhazy", "Fennimore", "Gaudreault",
        "Holmqvist", "Iacobelli", "Jaskolski", "Kowalewska", "Lindqvist-Oyama", "Mavromatis",
        "Nakashima-Roe", "Ostrowski", "Pellegrino-Vance", "Quarshie", "Radovanovic", "Szczepanik",
        "Thorvaldsen", "Uchendu", "Vukovich", "Wojciechowska", "Xanthopoulos", "Yamamura-Kell",
        "Zaborowski", "Abelardo-Finch", "Bergkvist", "Cieslak", "Dziedzic", "Engstrand",
        "Fiorentino-Lake", "Gruszka"]
PARENT_FIRST = ["Odalys", "Benedek", "Calloway", "Delphine", "Evander", "Fenella", "Gaspard",
                "Honorine", "Isidore", "Jessamyn", "Kasimir", "Leocadia"]
LANGS = ["English"] * 5 + ["Spanish", "Spanish", "Portuguese", "Haitian Creole", "Vietnamese"]

seeded = []

def seed(value, kind, file, column):
    seeded.append({"value": value, "kind": kind, "file": f"data/{file}", "column": column})

def dob_for(grade, year_start=2026):
    # Age 5 on Sept 1 for K.
    g = -1 if grade == "PK" else (0 if grade == "K" else int(grade))
    year = year_start - 6 - g + 1
    return dt.date(year, random.randint(1, 12), random.randint(1, 28))

# ---------------------------------------------------------------- screening roster
grades = ["PK", "K", "K", "1", "1", "2", "3", "4", "4", "5", "5", "6", "7", "7", "8", "9", "10",
          "10", "11", "12", "1", "4", "7", "10", "K", "3", "6", "9"]
rows = []
for i, g in enumerate(grades):
    sid = f"95{i + 10001:05d}"
    fn, ln = FIRST[i], LAST[i]
    dob = dob_for(g)
    sex = random.choice(["F", "M"])
    lang = LANGS[i % len(LANGS)]
    pname = f"{PARENT_FIRST[i % len(PARENT_FIRST)]} {ln}"
    phone = f"617-555-01{i + 10:02d}"
    new = "Y" if i in (5, 16) else ""
    r = {"Student ID": sid, "Last Name": ln, "First Name": fn, "Date of Birth": dob.isoformat(),
         "Sex": sex, "Grade": g, "Home Language": lang, "Parent Name": pname, "Parent Phone": phone,
         "New Student": new}
    def res(p_refer=0.15):
        x = random.random()
        if x < 0.08:
            return ""
        if x < 0.08 + p_refer:
            return "refer"
        if x < 0.12 + p_refer:
            return "absent"
        return "pass"
    sdate = dt.date(2026, 10, random.randint(1, 30)).isoformat()
    vr = res(0.2)
    r.update({"Vision Date": sdate if vr else "", "Vision Result": vr,
              "Vision Referral Completed": ("2026-11-0" + str(random.randint(1, 5))) if vr == "refer" and random.random() < 0.5 else ""})
    hr = res(0.1)
    r.update({"Hearing Date": sdate if hr else "", "Hearing Result": hr,
              "Hearing Referral Completed": "Y" if hr == "refer" and random.random() < 0.4 else ""})
    pr = res(0.1) if g in ("5", "6", "7", "8", "9") else ""
    r.update({"Postural Date": sdate if pr else "", "Postural Result": pr, "Postural Referral Completed": ""})
    if g in ("1", "4", "7", "10") and random.random() < 0.9:
        age = (dt.date(2026, 10, 15) - dob).days / 365.25
        height = round(35 + 2.6 * age + random.uniform(-2, 2), 1)
        bmi = random.uniform(14.5, 27.0)
        weight = round(bmi * height * height / 703, 1)
        r.update({"Height (in)": height, "Weight (lb)": weight, "BMI Date": "2026-10-15"})
    else:
        r.update({"Height (in)": "", "Weight (lb)": "", "BMI Date": ""})
    r["K Vision Certificate"] = ("on file" if random.random() < 0.7 else "") if g == "K" else ""
    rows.append(r)
    seed(sid, "other_id_number", "school_screenings.csv", "Student ID")
    seed(ln, "name", "school_screenings.csv", "Last Name")
    seed(fn, "name", "school_screenings.csv", "First Name")
    seed(dob.isoformat(), "birth_date", "school_screenings.csv", "Date of Birth")
    seed(pname, "name", "school_screenings.csv", "Parent Name")
    seed(phone, "phone", "school_screenings.csv", "Parent Phone")
with open(OUT / "school_screenings.csv", "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
    w.writeheader()
    w.writerows(rows)

# ---------------------------------------------------------------- absence log
schools = ["Aldergrove Elementary", "Quarry Hill Middle"]
pool = [(f"95{20001 + i:05d}", f"{FIRST[(i + 7) % 32]} {LAST[(i + 11) % 32]}", schools[i % 2],
         str(random.randint(1, 5) if i % 2 == 0 else random.randint(6, 8))) for i in range(40)]
reasons_base = ["fever and cough", "cold", "sore throat", "vomiting", "stomach ache", "sick",
                "doctor appointment", "family trip", "headache", "flu", "diarrhea", "unexcused"]
abs_rows = []
day = dt.date(2026, 9, 28)
school_days = []
while len(school_days) < 26:
    if day.weekday() < 5:
        school_days.append(day)
    day += dt.timedelta(days=1)
used = set()
for n, d in enumerate(school_days):
    for school in schools:
        members = [p for p in pool if p[2] == school]
        k = random.randint(3, 7)
        chosen = random.sample(members, k)
        for p in chosen:
            reason = random.choice(reasons_base)
            if "vomit" in reason or "diarr" in reason or "stomach" in reason:
                if random.random() < 0.6:
                    reason = random.choice(["cold", "doctor appointment", "family trip"])
            abs_rows.append([d.isoformat(), school, p[3], p[0], p[1], reason])
            used.add(p)
        if school == "Aldergrove Elementary" and n >= len(school_days) - 2:
            for p in random.sample(members, 12):
                abs_rows.append([d.isoformat(), school, p[3], p[0], p[1],
                                 random.choice(["vomiting", "vomiting and diarrhea", "stomach bug"])])
                used.add(p)
with open(OUT / "school_absence_log.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["Date", "School", "Grade", "Student ID", "Student Name", "Reason"])
    w.writerows(abs_rows)
for p in sorted(used):
    seed(p[0], "other_id_number", "school_absence_log.csv", "Student ID")
    seed(p[1], "name", "school_absence_log.csv", "Student Name")

# ---------------------------------------------------------------- visit log
visit_reasons = [("headache", "returned to class"), ("stomach ache", "returned to class"),
                 ("fell at recess", "returned to class"), ("scheduled medication", "returned to class"),
                 ("fever", "dismissed"), ("vomiting", "sent home"), ("anxiety", "returned to class"),
                 ("asthma - inhaler", "returned to class"), ("cut on finger", "returned to class"),
                 ("sore throat", "returned to class"), ("glucose check", "returned to class"),
                 ("twisted ankle in PE", "sent home"), ("stress", "referred to counselor"),
                 ("lost tooth", "returned to class")]
vpool = [(f"95{30001 + i:05d}", f"{FIRST[(i + 3) % 32]} {LAST[(i + 19) % 32]}", str(1 + i % 8)) for i in range(36)]
vrows = []
d = dt.date(2025, 9, 2)
vused = set()
while d <= dt.date(2026, 6, 19):
    if d.weekday() < 5 and not (dt.date(2025, 12, 24) <= d <= dt.date(2026, 1, 1)):
        for _ in range(random.randint(2, 6)):
            p = random.choice(vpool)
            reason, disp = random.choice(visit_reasons)
            vrows.append([d.isoformat(), p[0], p[1], p[2], "student", reason, disp, "", ""])
            vused.add(p)
        if random.random() < 0.15:
            vrows.append([d.isoformat(), "", "", "", "staff", "headache", "returned to work", "", ""])
    d += dt.timedelta(days=1)
# A few emergencies and head injuries.
for (date, reason, disp, ems, head) in [("2025-10-14", "head injury at recess", "sent home", "", "Y"),
                                        ("2025-11-20", "allergic reaction", "911", "medical", ""),
                                        ("2026-02-03", "panic attack", "911", "behavioral", ""),
                                        ("2026-03-11", "head injury in PE", "returned to class", "", "Y"),
                                        ("2026-04-22", "seizure", "911", "medical", "")]:
    p = random.choice(vpool)
    vrows.append([date, p[0], p[1], p[2], "student", reason, disp, ems, head])
    vused.add(p)
vrows.sort(key=lambda r: r[0])
with open(OUT / "health_office_visits.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["Date", "Student ID", "Student Name", "Grade", "Type", "Reason", "Disposition", "911 Type", "Head Injury"])
    w.writerows(vrows)
for p in sorted(vused):
    seed(p[0], "other_id_number", "health_office_visits.csv", "Student ID")
    seed(p[1], "name", "health_office_visits.csv", "Student Name")

# ---------------------------------------------------------------- concussion log
crow = []
cfile = "concussion_log.csv"
events = [
    ("2026-09-18", "Soccer", "Y", "2026-09-21", "2026-09-21", "Y", "Y", "2026-09-22", "2026-09-29", "2026-10-02", "2026-10-05", "Physician", "2026-10-12", "Spanish"),
    ("2026-09-25", "Football", "Y", "2026-09-30", "2026-09-26", "Y", "Y", "", "", "", "2026-10-03", "Coach", "2026-10-06", "English"),
    ("2026-10-02", "Field Hockey", "Y", "2026-10-05", "2026-10-05", "Y", "Y", "2026-10-06", "", "", "", "", "", "Portuguese"),
    ("2026-10-09", "Cheerleading", "Y", "2026-10-13", "", "N", "N", "", "", "", "", "", "", "English"),
    ("2026-10-16", "Ice Hockey", "Y", "2026-10-19", "2026-10-19", "Y", "Y", "2026-10-20", "2026-10-26", "2026-10-28", "2026-10-30", "Athletic Trainer", "", "English"),
    ("2026-10-20", "Recess (not athletics)", "N", "", "", "Y", "N", "", "", "", "", "", "", "Vietnamese"),
]
for i, e in enumerate(events):
    sid = f"95{40001 + i:05d}"
    name = f"{FIRST[(i + 21) % 32]} {LAST[(i + 5) % 32]}"
    pname = f"{PARENT_FIRST[(i + 4) % 12]} {LAST[(i + 5) % 32]}"
    email = f"{PARENT_FIRST[(i + 4) % 12].lower()}.family{i + 1}@example.org"
    grade = str(9 + i % 4)
    crow.append([name, sid, grade, e[1], e[13], pname, email, e[0], e[2], "Y" if e[2] == "Y" else "", e[3], e[4], e[5], e[6], e[7], e[8], e[9], e[10], e[11], e[12]])
    seed(sid, "other_id_number", cfile, "Student ID")
    seed(name, "name", cfile, "Student Name")
    seed(pname, "name", cfile, "Parent Name")
    seed(email, "email", cfile, "Parent Email")
with open(OUT / cfile, "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["Student Name", "Student ID", "Grade", "Sport", "Home Language", "Parent Name", "Parent Email",
                "Injury Date", "Extracurricular", "Removed From Play", "Written Notice", "AD Nurse Notified",
                "Head Injury Form", "Diagnosed Concussion", "Reentry Plan", "Symptom Free", "Full Academics",
                "Clearance Date", "Cleared By", "Full Return"])
    w.writerows(crow)

# ---------------------------------------------------------------- health plan students
pfile = "health_plan_students.csv"
plans = [("diabetes", "", "Glucagon nasal 3 mg", "", "80-180 mg/dL", "70 mg/dL"),
         ("asthma", "dust, cold air", "Albuterol 2 puffs", "", "", ""),
         ("seizures", "", "Diazepam nasal 10 mg", "", "", ""),
         ("food allergy", "", "Epinephrine 0.3 mg auto-injector", "peanut, tree nuts", "", ""),
         ("food allergy", "", "Epinephrine 0.15 mg auto-injector", "egg", "", "")]
prow = []
for i, p in enumerate(plans):
    sid = f"95{50001 + i:05d}"
    name = f"{FIRST[(i + 13) % 32]} {LAST[(i + 25) % 32]}"
    g = str(2 + i * 2)
    dob = dob_for(g)
    pname = f"{PARENT_FIRST[(i + 7) % 12]} {LAST[(i + 25) % 32]}"
    phone = f"508-555-01{40 + i:02d}"
    lang = ["Spanish", "English", "Portuguese", "English", "Haitian Creole"][i]
    prow.append([name, sid, dob.isoformat(), g, "Example Elementary", pname, phone, lang, p[0], "Dr. Provider Example", "617-555-0199",
                 p[2], p[1], p[3], p[4], p[5]])
    seed(sid, "other_id_number", pfile, "Student ID")
    seed(name, "name", pfile, "Student Name")
    seed(dob.isoformat(), "birth_date", pfile, "Date of Birth")
    seed(pname, "name", pfile, "Parent Name")
    seed(phone, "phone", pfile, "Parent Phone")
with open(OUT / pfile, "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["Student Name", "Student ID", "Date of Birth", "Grade", "School", "Parent Name", "Parent Phone",
                "Home Language", "Condition", "Provider Name", "Provider Phone", "Emergency Medication",
                "Triggers", "Allergens", "Target Range", "Low Threshold"])
    w.writerows(prow)

json.dump(seeded, sys.stdout, indent=2)
