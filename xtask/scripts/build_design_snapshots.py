#!/usr/bin/env python3
"""Build the Phase 16 Healthy People 2030 snapshot (PLAN-02 write_smartie_objectives).

Usage (from the repository root, Python 3.9+, standard library only):

    python3 -I xtask/scripts/build_design_snapshots.py [--check] [--workers N]

Source: the Healthy People 2030 web site of the Office of Disease Prevention and Health Promotion
(ODPHP), https://odphp.health.gov/healthypeople. The site has no public JSON API that could be found
from the build network (/healthypeople/api and similar paths return 404; health.gov itself is
blocked), so the builder reads the site's own server-rendered pages:

1. the "Browse objectives by topic" index and every topic page, which list their objectives;
2. every objective page found that way, which carries the objective's title and ID, national
   status, most recent data, national target, desired direction, national baseline (with its
   year), target-setting method, data source, topics, and workgroup.

Nothing is filled from memory. A page whose target or baseline cannot be read is kept with those
fields empty (the objective still appears, flagged), and the builder prints a count of them.

It writes snapshots/healthy_people_2030_objectives.json and updates the manifest entry
"healthy-people-2030-objectives" (checksum, rows, derivation). With --check it writes nothing and
reports whether the file would change (Healthy People data are updated through the year, so a
change after a few months is expected, not an error).

Limits, repeated in the snapshot and in the tool's caveats:

* Objectives the topic pages do not list are missing; the count is in the file.
* Subpopulation (disparities) data, state data, and the "Data" tab charts are not included: only
  the national header of each objective page.
* Values are as shown on the page on the retrieval date; ODPHP revises baselines and targets
  ("Baseline has been revised" is kept as a flag).
"""

import concurrent.futures
import datetime
import hashlib
import html
import json
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
UA = "phn-mcp-server snapshot builder (+https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server)"
HOST = "https://odphp.health.gov"
BROWSE = HOST + "/healthypeople/objectives-and-data/browse-objectives"
LHI = HOST + "/healthypeople/objectives-and-data/leading-health-indicators"
FILE = "healthy_people_2030_objectives.json"
SNAP_ID = "healthy-people-2030-objectives"

NOT_INCLUDED = [
    "Objectives that no topic page lists are not included; the number found is in the file.",
    "Subpopulation (disparities) data, state data, and the data tables and charts behind each objective are not included: only the national status, most recent value, target, baseline, and data source from each objective page.",
    "Values are as shown on the page on the retrieval date. ODPHP revises baselines and targets and adds data through the year; check the objective page before you cite a figure in a grant or a report.",
]


def sha(b):
    return hashlib.sha256(b).hexdigest()


def get(url, tries=4):
    last = None
    for i in range(tries):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": UA})
            with urllib.request.urlopen(req, timeout=40) as r:
                return r.read().decode("utf-8", "replace")
        except urllib.error.HTTPError as e:
            if e.code == 404:
                raise
            last = e
        except Exception as e:  # noqa: BLE001
            last = e
        time.sleep(1.5 * (i + 1))
    raise last


def text_of(fragment):
    fragment = re.sub(r"<(script|style)\b.*?</\1>", "", fragment, flags=re.S)
    fragment = re.sub(r"<[^>]+>", " ", fragment)
    fragment = html.unescape(fragment).replace("‑", "-").replace(" ", " ")
    return re.sub(r"\s+", " ", fragment).strip()


def num(s):
    """First number in a string (commas allowed), or None."""
    m = re.search(r"-?\d[\d,]*\.?\d*", s or "")
    if not m:
        return None
    try:
        return float(m.group(0).replace(",", ""))
    except ValueError:
        return None


TOPIC_RE = re.compile(r'href="(/healthypeople/objectives-and-data/browse-objectives/([a-z0-9-]+))"')
OBJ_RE = re.compile(r'href="(/healthypeople/objectives-and-data/browse-objectives/([a-z0-9-]+)/([a-z0-9-]+))"')
SKIP_LEAF = {"evidence-based-resources", "data", "data-methodology", "healthy-people-in-action"}


def topics():
    page = get(BROWSE)
    seen = []
    for _path, slug in TOPIC_RE.findall(page):
        if slug not in seen:
            seen.append(slug)
    return seen


def objective_links(topic):
    page = get(f"{BROWSE}/{topic}")
    out = []
    for path, _t, leaf in OBJ_RE.findall(page):
        if leaf in SKIP_LEAF:
            continue
        out.append((leaf, path))
    return out


def block(page, name):
    """HTML of one data-header block (recent, target, direction, baseline)."""
    m = re.search(r'<div class="o-dataheader-' + name + r'\b[^>]*>(.*?)(?=<div class="o-data|<div class="o-objective|$)', page, flags=re.S)
    return m.group(1) if m else ""


def value_unit(fragment):
    """(value text, unit text, flags) from one header block."""
    flags = []
    for title in re.findall(r'<sup>\s*<a[^>]*title="([^"]+)"', fragment):
        flags.append(title.split(".")[0].strip().lower())
    f = re.sub(r"<small>.*?</small>", " ", fragment, flags=re.S)
    f = re.sub(r"<sup>.*?</sup>", " ", f, flags=re.S)
    f = re.sub(r'<span class="link-baseline-age-adjustment">.*?</span>', " ", f, flags=re.S)
    f = re.sub(r"<img[^>]*>", " ", f)
    return text_of(f), flags


def parse_objective(page, url):
    h1 = re.search(r'<h1 class="page-title">(.*?)</h1>', page, flags=re.S)
    if not h1:
        raise ValueError("no title")
    title_full = text_of(h1.group(1))
    m = re.match(r"^(.*?)\s+[—–-]\s+([A-Z][A-Z0-9/-]*)$", title_full)
    if not m:
        raise ValueError(f"no objective id in title {title_full!r}")
    title, oid = m.group(1).strip(), m.group(2)
    flat = text_of(page)
    st = re.search(r"National Status:\s*(.+?)\s+Learn more about our data release schedule", flat)
    status = st.group(1).strip() if st else None
    rec_text, _ = value_unit(block(page, "recent"))
    tgt_text, tgt_flags = value_unit(block(page, "target"))
    dir_text = text_of(re.sub(r"<small>.*?</small>", " ", block(page, "direction"), flags=re.S))
    base_text, base_flags = value_unit(block(page, "baseline"))
    tsm = re.search(r"Target-Setting Method</div>\s*<div class=\"field__item\">(.*?)</div>", page, flags=re.S)
    ds = re.search(r'o-objective-data-sources">.*?<a href="([^"]+)">(.*?)</a>', page, flags=re.S)
    topics_blk = re.search(r"Topics?:&nbsp;</strong>(.*?)</p>", page, flags=re.S)
    topic_names = [text_of(x) for x in re.findall(r">([^<]+)</a>", topics_blk.group(1))] if topics_blk else []
    wg = re.search(r"Workgroup:&nbsp;</strong>\s*<a[^>]*>(.*?)</a>", page, flags=re.S)
    age_adj = bool(re.search(r"Age adjusted to the year 2000 standard population", page))

    # Most recent: "66.0 percent (2023)"  or "6.4 new cases ... per 1,000 adults (2022-24)"
    period = None
    mp = re.search(r"\((\d{4}(?:-\d{2,4})?)\)\s*$", rec_text)
    if mp:
        period = mp.group(1)
    # Baseline: "64.8 percent of adults ... in 2019" / "... as reported in 2019-21"
    base_year = None
    by = re.search(r"\b(?:in|reported in|for|during)\s+((?:19|20)\d{2}(?:-\d{2,4})?)\s*$", base_text)
    if by:
        base_year = by.group(1)
    objective_type = "core"
    if status is None:
        objective_type = "unknown"
    elif status == "Developmental":
        objective_type = "developmental"
    elif status == "Research":
        objective_type = "research"
    return {
        "id": oid,
        "title": title,
        "url": url,
        "type": objective_type,
        "status": status,
        "topics": topic_names,
        "workgroup": text_of(wg.group(1)) if wg else None,
        "desired_direction": dir_text or None,
        "most_recent_text": rec_text or None,
        "most_recent_value": num(rec_text),
        "most_recent_period": period,
        "target_text": tgt_text or None,
        "target_value": num(tgt_text),
        "baseline_text": base_text or None,
        "baseline_value": num(base_text),
        "baseline_year": base_year,
        "age_adjusted": age_adj,
        "revised": sorted(set(tgt_flags + base_flags)),
        "target_setting_method": text_of(tsm.group(1)) if tsm else None,
        "data_source": text_of(ds.group(2)) if ds else None,
        "data_source_url": (HOST + ds.group(1)) if ds and ds.group(1).startswith("/") else (ds.group(1) if ds else None),
        "leading_health_indicator": False,
    }


def build(workers):
    tps = topics()
    print(f"{len(tps)} topics", file=sys.stderr)
    links = {}
    with concurrent.futures.ThreadPoolExecutor(workers) as ex:
        for topic, found in zip(tps, ex.map(objective_links, tps)):
            for leaf, path in found:
                links.setdefault(leaf, path)
    print(f"{len(links)} distinct objective pages", file=sys.stderr)

    def one(item):
        leaf, path = item
        url = HOST + path
        try:
            return parse_objective(get(url), url)
        except Exception as e:  # noqa: BLE001
            return {"error": f"{url}: {e}"}

    results = []
    with concurrent.futures.ThreadPoolExecutor(workers) as ex:
        results = list(ex.map(one, sorted(links.items())))
    errors = [r["error"] for r in results if "error" in r]
    objs = [r for r in results if "error" not in r]
    if errors:
        print(f"{len(errors)} pages could not be read:", file=sys.stderr)
        for e in errors[:20]:
            print("  ", e, file=sys.stderr)
    by_id = {}
    for o in objs:
        by_id.setdefault(o["id"], o)
    # Leading health indicators: the LHI page links to each LHI objective's page.
    lhi_ids = set()
    try:
        lhi_leaves = {leaf for _p, _t, leaf in OBJ_RE.findall(get(LHI)) if leaf not in SKIP_LEAF}
        for o in by_id.values():
            if o["url"].rsplit("/", 1)[-1] in lhi_leaves:
                o["leading_health_indicator"] = True
                lhi_ids.add(o["id"])
    except Exception as e:  # noqa: BLE001
        print("LHI page not read:", e, file=sys.stderr)
    final = [by_id[k] for k in sorted(by_id)]
    return final, len(links), errors, sorted(lhi_ids & set(by_id))


def main():
    check = "--check" in sys.argv
    workers = 4
    if "--workers" in sys.argv:
        workers = int(sys.argv[sys.argv.index("--workers") + 1])
    objs, n_links, errors, lhis = build(workers)
    if not objs:
        sys.exit("no objectives read")
    if len(errors) > max(5, len(objs) // 20):
        sys.exit("too many pages failed; refusing to write")
    path = SNAP / FILE
    manifest_path = SNAP / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    old_entry = next((e for e in manifest["snapshots"] if e["id"] == SNAP_ID), None)
    today = datetime.date.today().isoformat()
    if check and old_entry:
        today = old_entry["retrieved"]
    counts = {}
    for o in objs:
        counts[o["type"]] = counts.get(o["type"], 0) + 1
    doc = {
        "format_version": 1,
        "description": "Healthy People 2030 objectives read from ODPHP's objective pages (odphp.health.gov/healthypeople): national status, most recent data, target, baseline, data source.",
        "retrieved": today,
        "pages_found": n_links,
        "pages_unreadable": len(errors),
        "counts_by_type": counts,
        "leading_health_indicator_ids": lhis,
        "not_included": NOT_INCLUDED,
        "objectives": objs,
    }
    text = json.dumps(doc, indent=1, ensure_ascii=False) + "\n"
    new = text.encode("utf-8")
    old = path.read_bytes() if path.exists() else b""
    missing_target = sum(1 for o in objs if o["type"] == "core" and o["target_value"] is None)
    missing_baseline = sum(1 for o in objs if o["type"] == "core" and o["baseline_value"] is None)
    print(f"{FILE}: sha256 {sha(new)}, bytes {len(new)}, objectives {len(objs)} {counts}; core without target {missing_target}, without baseline {missing_baseline}; LHI {len(lhis)}")
    if check:
        print("unchanged" if old == new else "changed")
        sys.exit(0 if old == new else 1)
    path.write_bytes(new)
    entry = {
        "id": SNAP_ID,
        "title": "Healthy People 2030 objectives: national status, most recent data, target, and baseline",
        "publisher": "U.S. Department of Health and Human Services, Office of Disease Prevention and Health Promotion (ODPHP), Healthy People 2030",
        "source_urls": [
            BROWSE,
            BROWSE + "/<topic>/<objective> (one page per objective; the builder lists them from the topic pages)",
            LHI,
        ],
        "source_sha256": {},
        "source_vintage": f"pages read {today}; most recent data years as shown on each page",
        "retrieved": today,
        "license": "Healthy People 2030 content is a U.S. government work (public domain); cite ODPHP.",
        "derivation": "Server-rendered objective pages on odphp.health.gov read by xtask/scripts/build_design_snapshots.py (Python 3 standard library): the browse index and topic pages give the objective links; each objective page gives the id, title, status, most recent data, national target, desired direction, national baseline (with its year), target-setting method, data source, topics, and workgroup, copied as text with numbers parsed from the same text. No JSON API for Healthy People 2030 could be found (odphp.health.gov/healthypeople/api returns 404 and health.gov is blocked from the build network).",
        "caveats": NOT_INCLUDED,
        "files": [{"path": FILE, "sha256": sha(new), "bytes": len(new), "rows": len(objs)}],
    }
    snaps = [e for e in manifest["snapshots"] if e["id"] != SNAP_ID]
    snaps.append(entry)
    manifest["snapshots"] = snaps
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print("manifest updated")


if __name__ == "__main__":
    main()
