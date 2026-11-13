#!/usr/bin/env python3
"""Build the Phase 19 legislation snapshot (POL-01 track_ma_legislation).

Usage (from the repository root, Python 3.9+, standard library only):

    python3 -I xtask/scripts/build_tracking_snapshots.py [--date YYYY-MM-DD] [--workers N]
    python3 -I xtask/scripts/build_tracking_snapshots.py --check

What it reads (public, no key; the requests carry only a General Court number and bill numbers):

* https://malegislature.gov/api/GeneralCourts/194/Documents  (every bill and docket of the 194th
  General Court, 2025-2026; about 12 MB)
* https://malegislature.gov/api/GeneralCourts/194/Documents/{bill}  (title, pinslip, text,
  committee recommendations) and .../DocumentHistoryActions  (the bill's history), for each bill
  whose TITLE matches a topic keyword below.

It writes snapshots/ma_legislation_194.json and updates its entry in snapshots/manifest.json.
Rows are public records of the Massachusetts Legislature. Sponsors' and amendment authors' names are not kept (the bill page lists them). Bill text
is not copied except one excerpt of at most 25 words per bill.

The TOPICS list below is the same list as content/policy/ma_legislation_tracking.yaml (a Rust
test checks that they agree). Matching: a keyword matches when its words appear in order in the
title, ignoring case and punctuation, with a trailing plural "s" removed from words longer than
three letters (so "vaccines" matches "vaccine"), exactly as phn-tools' matching does.

Because the Legislature changes the data every day, --check does not compare with a fresh download.
It verifies the file on disk: checksum against the manifest, that every bill's topics follow from
its title, that every excerpt is 25 words or fewer, and that bill numbers are unique.
"""

import concurrent.futures as cf
import datetime
import hashlib
import json
import re
import sys
import time
import unicodedata
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
FILE = "ma_legislation_194.json"
SNAP_ID = "ma-legislation-194"
GC = 194
BASE = "https://malegislature.gov/api"
UA = "phn-mcp-server-snapshot-builder"
MAX_WORDS = 25

TOPICS = [
    {"id": "vaccines", "label": "Vaccines and immunization", "keywords": ["vaccine", "vaccination", "immunization", "immunize", "immunisation"]},
    {"id": "communicable_disease", "label": "Communicable disease and infection control", "keywords": ["communicable disease", "infectious disease", "disease control", "tuberculosis", "measles", "sexually transmitted", "hiv", "hepatitis", "outbreak", "infection control", "contact tracing"]},
    {"id": "tobacco_nicotine", "label": "Tobacco, nicotine, and vaping", "keywords": ["tobacco", "nicotine", "vape", "vaping", "e-cigarette", "electronic cigarette", "cigarette", "menthol"]},
    {"id": "substance_use", "label": "Substance use, overdose, and harm reduction", "keywords": ["opioid", "overdose", "substance use", "substance use disorder", "naloxone", "narcan", "harm reduction", "addiction", "fentanyl", "drug checking", "syringe"]},
    {"id": "mental_health", "label": "Mental and behavioral health", "keywords": ["mental health", "behavioral health", "suicide", "psychiatric", "crisis response", "988", "mental illness"]},
    {"id": "maternal_child", "label": "Maternal, infant, and child health", "keywords": ["maternal", "postpartum", "perinatal", "doula", "midwife", "midwives", "newborn", "pregnancy", "pregnant", "prenatal", "infant", "breastfeeding", "lactation", "early intervention", "birth center"]},
    {"id": "school_child_care", "label": "School health and child care", "keywords": ["school nurse", "school health", "student health", "school-based health", "school based health", "child care", "childcare", "school meals", "school lunch"]},
    {"id": "older_adults", "label": "Older adults and long-term care", "keywords": ["older adult", "elder", "aging", "nursing home", "long-term care", "long term care", "assisted living", "dementia", "alzheimer", "home care", "caregiver"]},
    {"id": "environmental_health", "label": "Environmental health and climate", "keywords": ["pfas", "lead paint", "lead poisoning", "lead service", "drinking water", "air quality", "extreme heat", "heat emergency", "climate", "pesticide", "environmental justice", "mold", "sanitary code", "wastewater"]},
    {"id": "food_nutrition", "label": "Food security and nutrition", "keywords": ["food insecurity", "nutrition", "snap", "hunger", "food safety", "food allergy", "food allergen", "sugar sweetened", "school breakfast"]},
    {"id": "local_public_health", "label": "Local and state public health system", "keywords": ["board of health", "local public health", "public health nurse", "public health workforce", "public health excellence", "health department", "department of public health", "public health emergency", "public health"]},
    {"id": "nursing_workforce", "label": "Nursing practice and workforce", "keywords": ["nurse", "nursing", "nurse staffing", "nurse licensure", "nurse practitioner", "health care workforce", "community health worker", "health care worker"]},
    {"id": "health_equity", "label": "Health equity and language access", "keywords": ["health equity", "language access", "interpreter", "health disparities", "racial disparities", "immigrant health", "health care access"]},
    {"id": "oral_health", "label": "Oral health", "keywords": ["dental", "oral health", "fluoride", "fluoridation", "dentist"]},
    {"id": "emergency_preparedness", "label": "Emergency preparedness and response", "keywords": ["emergency preparedness", "disaster", "emergency response", "pandemic", "public health emergency", "emergency medical"]},
    {"id": "health_coverage", "label": "Health coverage and telehealth", "keywords": ["masshealth", "medicaid", "health insurance", "telehealth", "telemedicine", "prior authorization", "health care coverage"]},
    {"id": "reproductive_sexual_health", "label": "Reproductive and sexual health", "keywords": ["contraception", "contraceptive", "reproductive health", "family planning", "sexual health", "sex education"]},
    {"id": "injury_safety", "label": "Injury prevention and safety", "keywords": ["injury prevention", "drowning", "fall prevention", "concussion", "poison control", "violence prevention"]},
]


def norm(s):
    out = []
    last = True
    for c in unicodedata.normalize("NFKD", s):
        if unicodedata.combining(c):
            continue
        if c.isalnum():
            out.append(c.lower())
            last = False
        elif c in "'’":
            continue
        elif not last:
            out.append(" ")
            last = True
    return "".join(out).strip()


def stem(s):
    r = []
    for w in norm(s).split():
        if w.endswith("s") and len(w) > 3 and not w.endswith("ss"):
            w = w[:-1]
        r.append(w)
    return r


def has_phrase(words, phrase):
    p = stem(phrase)
    if not p or len(p) > len(words):
        return False
    return any(words[i:i + len(p)] == p for i in range(len(words) - len(p) + 1))


def topics_for(title):
    w = stem(title)
    return [t["id"] for t in TOPICS if any(has_phrase(w, k) for k in t["keywords"])]


def excerpt(text, keywords):
    """At most MAX_WORDS words of the text around the first keyword, with ellipses."""
    if not text:
        return None
    words = text.split()
    if not words:
        return None
    best = None
    for k in keywords:
        m = re.search(r"\b" + re.escape(k), text, re.I)
        if m and (best is None or m.start() < best):
            best = m.start()
    idx = 0
    if best is not None:
        idx = len(text[:best].split())
        idx = max(0, idx - 8)
    chunk = words[idx:idx + MAX_WORDS]
    s = " ".join(chunk)
    return ("… " if idx > 0 else "") + s + (" …" if idx + MAX_WORDS < len(words) else "")


def words_in(excerpt_text):
    return len(excerpt_text.replace("…", " ").split())


def get(url, tries=4):
    err = None
    for i in range(tries):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": UA})
            with urllib.request.urlopen(req, timeout=120) as r:
                return json.load(r)
        except Exception as e:  # noqa: BLE001
            err = e
            time.sleep(1 + 2 * i)
    raise RuntimeError(f"{url}: {err}")


AMEND = re.compile(r"^(Amendment\s+#?\d+[A-Za-z]?)\s*\([^)]*\)")


def omit_member(action):
    """Amendment lines name the member who offered the amendment; the names are not kept."""
    return AMEND.sub(r"\1 (member omitted)", action)


def clean(s):
    return " ".join(s.split()) if isinstance(s, str) else s


def one(item):
    number, topics = item["BillNumber"], item["_topics"]
    d = get(f"{BASE}/GeneralCourts/{GC}/Documents/{number}")
    h = get(f"{BASE}/GeneralCourts/{GC}/Documents/{number}/DocumentHistoryActions")
    kws = [k for t in TOPICS if t["id"] in topics for k in t["keywords"]]
    history = []
    for a in h:
        date = (a.get("Date") or "")[:10]
        if not re.fullmatch(r"(19|20|21)\d\d-\d\d-\d\d", date):
            continue
        history.append([date, clean(a.get("Branch") or ""), omit_member(clean(a.get("Action") or "")), bool(a.get("IsStricken"))])
    history.sort(key=lambda x: x[0])
    recs = []
    for r in d.get("CommitteeRecommendations") or []:
        recs.append([(r.get("Committee") or {}).get("CommitteeCode") or "", clean(r.get("Action") or "")])
    return {
        "bill": number,
        "docket": item.get("DocketNumber"),
        "title": clean(item["Title"]),
        "cosponsors": len(item.get("Cosponsors") or []),
        "type": d.get("LegislationTypeName"),
        "topics": topics,
        "recommendations": recs,
        "excerpt": excerpt(" ".join((d.get("DocumentText") or "").split()), kws),
        "history": history,
    }


def sha(b):
    return hashlib.sha256(b).hexdigest()


def build(date, workers):
    docs = get(f"{BASE}/GeneralCourts/{GC}/Documents")
    picked = []
    for item in docs:
        if not item.get("BillNumber") or not item.get("Title"):
            continue
        t = topics_for(item["Title"])
        if t:
            item["_topics"] = t
            picked.append(item)
    picked.sort(key=lambda x: (x["BillNumber"][0], int(x["BillNumber"][1:])))
    with cf.ThreadPoolExecutor(workers) as ex:
        bills = list(ex.map(one, picked))
    out = {
        "general_court": GC,
        "years": "2025-2026",
        "as_of": date,
        "documents_in_list": len(docs),
        "bills_with_numbers": sum(1 for x in docs if x.get("BillNumber")),
        "topic_definitions": TOPICS,
        "bills": bills,
    }
    return json.dumps(out, ensure_ascii=False, separators=(",", ":")) + "\n", len(bills)


def entry(date, text, n):
    return {
        "id": SNAP_ID,
        "title": "Massachusetts bills of the 194th General Court (2025-2026) whose titles match public health topics",
        "publisher": "Massachusetts Legislature (public API at malegislature.gov/api)",
        "source_urls": [
            f"{BASE}/GeneralCourts/{GC}/Documents",
            f"{BASE}/GeneralCourts/{GC}/Documents/{{bill}}",
            f"{BASE}/GeneralCourts/{GC}/Documents/{{bill}}/DocumentHistoryActions",
        ],
        "source_sha256": {},
        "source_vintage": f"live API responses on {date}",
        "retrieved": date,
        "license": "Public records of the Massachusetts Legislature; the API's terms were not found in its Swagger document. Bill text is not copied except one excerpt of at most 25 words per bill.",
        "derivation": "xtask/scripts/build_tracking_snapshots.py downloads the 194th General Court document list, keeps bills that have a bill number and whose title matches a keyword of the topic list in the script (the same list as content/policy/ma_legislation_tracking.yaml), then fetches each bill's record and history. Per bill it keeps the number, docket, title, cosponsor count (sponsor names and amendment authors' names are not kept), topics, committee recommendations, the full dated history, and one excerpt of at most 25 words around the first topic keyword in the bill text.",
        "caveats": [
            "A bill whose title does not contain a topic keyword is not included, even if its text is about the topic.",
            "Status is as of the retrieval date; the Legislature updates it daily. Use live mode for current status.",
        ],
        "files": [{"path": FILE, "sha256": sha(text.encode()), "bytes": len(text.encode()), "rows": n}],
    }


def check():
    text = (SNAP / FILE).read_text()
    m = json.loads((SNAP / "manifest.json").read_text())
    e = next(x for x in m["snapshots"] if x["id"] == SNAP_ID)
    ok = True
    if e["files"][0]["sha256"] != sha(text.encode()):
        print("checksum differs from the manifest", file=sys.stderr)
        ok = False
    d = json.loads(text)
    if d["topic_definitions"] != TOPICS:
        print("topic definitions differ from this script's TOPICS", file=sys.stderr)
        ok = False
    seen = set()
    for b in d["bills"]:
        if b["bill"] in seen:
            print("duplicate", b["bill"], file=sys.stderr)
            ok = False
        seen.add(b["bill"])
        if b["topics"] != topics_for(b["title"]):
            print("topics do not follow from the title:", b["bill"], file=sys.stderr)
            ok = False
        if b["excerpt"] and words_in(b["excerpt"]) > MAX_WORDS:
            print("excerpt too long:", b["bill"], file=sys.stderr)
            ok = False
    sys.exit(0 if ok else 1)


def main():
    if "--check" in sys.argv:
        check()
    date = datetime.date.today().isoformat()
    if "--date" in sys.argv:
        date = sys.argv[sys.argv.index("--date") + 1]
    workers = 8
    if "--workers" in sys.argv:
        workers = int(sys.argv[sys.argv.index("--workers") + 1])
    text, n = build(date, workers)
    (SNAP / FILE).write_text(text)
    manifest_path = SNAP / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    snaps = [e for e in manifest["snapshots"] if e["id"] != SNAP_ID]
    snaps.append(entry(date, text, n))
    manifest["snapshots"] = snaps
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")
    print(f"wrote {FILE} ({n} bills, {len(text)} bytes)")


if __name__ == "__main__":
    main()
