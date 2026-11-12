#!/usr/bin/env python3
"""Build the Phase 16 evidence snapshot (PLAN-03 find_evidence_based_interventions).

Usage (from the repository root, Python 3.9+, standard library only):

    python3 -I xtask/scripts/build_interventions_snapshots.py [--check] [--date YYYY-MM-DD]

Why PubMed: the Community Guide (www.thecommunityguide.org) and County Health Rankings' What Works
for Health (www.countyhealthrankings.org) both answered the build network's CONNECT with 403 (the
egress policy blocks them), so neither site was read. The Community Preventive Services Task Force
and the Community Guide staff publish their systematic reviews, economic reviews, and Task Force
recommendation statements in journals, and PubMed indexes them under the Corporate Author
"Community Preventive Services Task Force" (and, before 2010, "Task Force on Community Preventive
Services"). This script reads those records through NCBI's E-utilities and keeps, for each one,
only what PubMed shows: title, journal, publication date, DOI, MeSH headings, what kind of paper
it is, any rating language in the abstract, and short excerpts of the abstract (the finding, the
conclusion, and any implementation sentence). It does not guess a rating.

What the snapshot is NOT: it is not the Community Guide's list of recommendations. Many
recommendations (and every What Works for Health rating) have no PubMed record, older records may
have been updated or archived, and the abstract often does not state the Task Force's rating.
The tool says so and links the two sites for the nurse to check.

It writes snapshots/cpstf_publications.json and updates the manifest entry "cpstf-publications"
(checksum, rows, source checksum). With --check it writes nothing, compares the file the script
would write with the one on disk (using the manifest's retrieval date), and exits 1 if different.
"""

import datetime
import hashlib
import json
import re
import sys
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
UA = "phn-mcp-server snapshot builder (+https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server)"
EUTILS = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils"
FILE = "cpstf_publications.json"
SNAP_ID = "cpstf-publications"
QUERY = (
    '"Community Preventive Services Task Force"[Corporate Author] OR '
    '"Task Force on Community Preventive Services"[Corporate Author]'
)

MONTHS = {m: i for i, m in enumerate(["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"], 1)}
SKIP_MESH = {"Humans", "Male", "Female", "United States", "Cost-Benefit Analysis"}

NOT_INCLUDED = [
    "This is not the Community Guide's list of recommendations. It holds the journal papers PubMed attributes to the Community Preventive Services Task Force (and its earlier name), so a recommendation or review that was posted only on thecommunityguide.org, or whose paper PubMed does not attribute to the Task Force, is missing.",
    "Papers about methods, and news items, are left out. Recommendation statements are kept even when PubMed has no abstract for them; their rating is then not in this snapshot.",
    "A rating appears only when the abstract's own words give it. Many abstracts report the review's findings and not the Task Force's rating, and some papers cover several interventions with different ratings.",
    "Recommendations can be updated or archived after the paper was published. Check the Community Guide page before relying on an older record.",
    "What Works for Health (County Health Rankings) has no data here: its site could not be reached from the build network.",
]

SOURCE_URLS = [
    EUTILS + "/esearch.fcgi?db=pubmed (Corporate Author query in xtask/scripts/build_interventions_snapshots.py)",
    EUTILS + "/efetch.fcgi?db=pubmed (all PubMed IDs the query returns, one request)",
    "https://www.thecommunityguide.org/ (not reachable from the build network; linked for the nurse)",
]


def sha(b):
    return hashlib.sha256(b).hexdigest()


def get(endpoint, params):
    params = dict(params, tool="public-health-nursing-mcp-server")
    req = urllib.request.Request(f"{EUTILS}/{endpoint}.fcgi?" + urllib.parse.urlencode(params), headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def clean(s):
    return re.sub(r"\s+", " ", s).strip()


def sentences(text):
    return [s for s in re.split(r"(?<=[.!?])\s+(?=[A-Z0-9(])", clean(text)) if s]


def clip(text, limit):
    """At most `limit` characters, cut at a sentence end when possible, else at a word."""
    text = clean(text)
    if len(text) <= limit:
        return text
    out = ""
    for s in sentences(text):
        if len(out) + len(s) + (1 if out else 0) > limit:
            break
        out = (out + " " + s).strip()
    if out:
        return out
    cut = text[:limit].rsplit(" ", 1)[0]
    return cut.rstrip(",;:") + "..."


def kind_of(title):
    t = title.lower()
    if re.search(r"^methods for|methods for conducting|community guide methods|model for linking|priority topics", t):
        return "methods"
    if re.search(r"recommendation|recommended|recommends", t):
        return "recommendation"
    if "economic" in t:
        return "economic_review"
    if re.search(r"systematic review|meta-analysis|review|reviews of evidence|effectiveness|effects of|economics of|economic evaluation", t):
        return "systematic_review"
    return "other"


def title_signal(title):
    """What the title itself says about direction (only the clear cases)."""
    t = title.lower()
    if "recommendation against" in t or "recommended against" in t or "recommends against" in t:
        return "recommended_against"
    if re.search(r"\b(is|are) recommended\b|\brecommended (to|for)\b|recommended to prevent", t):
        return "recommended"
    return None


RATING_PATTERNS = [
    ("recommended_against", re.compile(r"recommend(?:s|ed|ation)?\s+against", re.I)),
    ("insufficient_evidence", re.compile(r"insufficient evidence", re.I)),
    ("strong_evidence", re.compile(r"\bstrong evidence", re.I)),
    ("sufficient_evidence", re.compile(r"(?<!in)\bsufficient evidence", re.I)),
    ("recommended", re.compile(r"task force[^.]*\brecommend(?:s|ed)?\b(?!\s+against)", re.I)),
]


def rating_language(abstract_sentences):
    terms, found = [], []
    for s in abstract_sentences:
        hit = [name for name, rx in RATING_PATTERNS if rx.search(s)]
        if re.search(r"task force", s, re.I) and not hit:
            # Mentions the Task Force without a rating word: kept as context only when it
            # reports a finding or determination.
            if re.search(r"determined|concluded|found|finding", s, re.I):
                found.append(s)
            continue
        if hit:
            found.append(s)
            for h in hit:
                if h not in terms:
                    terms.append(h)
    return terms, found[:3]


def parse(xml):
    root = ET.fromstring(xml)
    out = {}
    for art in root.findall(".//PubmedArticle"):
        pmid = art.findtext(".//MedlineCitation/PMID")
        title = clean("".join(art.find(".//ArticleTitle").itertext()))
        pd = art.find(".//Journal/JournalIssue/PubDate")
        y, m = pd.findtext("Year"), pd.findtext("Month")
        if not y:
            md = pd.findtext("MedlineDate") or ""
            mm = re.match(r"(\d{4})", md)
            y = mm.group(1) if mm else None
            m = None
        if not y:
            ad = art.find(".//ArticleDate")
            if ad is not None:
                y, m = ad.findtext("Year"), ad.findtext("Month")
        if m and not m.isdigit():
            m = MONTHS.get(m[:3])
        published = None
        if y:
            published = f"{int(y):04d}" + (f"-{int(m):02d}" if m else "")
        doi = None
        for i in art.findall(".//PubmedData/ArticleIdList/ArticleId"):
            if i.get("IdType") == "doi":
                doi = i.text
        secs = []
        for at in art.findall(".//Abstract/AbstractText"):
            secs.append(((at.get("Label") or "").upper(), clean("".join(at.itertext()))))
        mesh = []
        for d in art.findall(".//MeshHeadingList/MeshHeading/DescriptorName"):
            n = clean("".join(d.itertext()))
            if n not in SKIP_MESH and n not in mesh:
                mesh.append(n)
        out[pmid] = {
            "pmid": pmid, "title": title, "journal": art.findtext(".//Journal/ISOAbbreviation"),
            "published": published, "doi": doi, "sections": secs, "mesh": mesh,
        }
    return out


def section(secs, *labels):
    for lab, txt in secs:
        if any(lab.startswith(l) for l in labels):
            return txt
    return ""


def record(p):
    secs = p["sections"]
    full = " ".join(t for _, t in secs)
    sents = sentences(full)
    terms, found = rating_language(sents)
    kind = kind_of(p["title"])
    result = section(secs, "RESULTS", "EVIDENCE SYNTHESIS", "FINDINGS")
    concl = section(secs, "CONCLUSION")
    disc = section(secs, "DISCUSSION")
    if not secs:
        pass
    elif len(secs) == 1 and not secs[0][0]:
        # Unstructured abstract: its last two sentences stand in for the conclusion.
        concl = " ".join(sents[-2:])
    impl = [s for s in sentences(disc) if re.search(r"barrier|implement|reimburs|cost|applicab|feasib", s, re.I)]
    rec = {
        "pmid": p["pmid"],
        "title": p["title"],
        "journal": p["journal"],
        "published": p["published"],
        "doi": p["doi"],
        "kind": kind,
        "title_signal": title_signal(p["title"]),
        "has_abstract": bool(secs),
        "mesh": p["mesh"][:14],
        "rating_terms": terms,
        "rating_sentences": [clip(s, 380) for s in found],
        "finding": clip(result, 380) if result else None,
        "conclusion": clip(concl, 380) if concl else None,
        "implementation": clip(" ".join(impl[:2]), 380) if impl else None,
    }
    return rec


def build(date):
    es = json.loads(get("esearch", {"db": "pubmed", "term": QUERY, "retmax": "500", "retmode": "json"}))["esearchresult"]
    ids = sorted(es["idlist"], key=int)
    if int(es["count"]) != len(ids):
        sys.exit(f"esearch count {es['count']} != ids {len(ids)}")
    xml = get("efetch", {"db": "pubmed", "id": ",".join(ids), "retmode": "xml"})
    parsed = parse(xml)
    missing = [i for i in ids if i not in parsed]
    if missing:
        sys.exit(f"PubMed did not return: {missing}")
    recs, dropped = [], {"methods": 0, "other": 0}
    for i in ids:
        r = record(parsed[i])
        if r["kind"] in dropped:
            dropped[r["kind"]] += 1
            continue
        if not r["published"]:
            sys.exit(f"{i}: no publication date")
        recs.append(r)
    recs.sort(key=lambda r: (r["published"], r["pmid"]), reverse=True)
    doc = {
        "format_version": 1,
        "description": "Journal papers PubMed attributes to the Community Preventive Services Task Force (Community Guide systematic reviews, economic reviews, and Task Force recommendation statements). Titles, dates, and rating language are as PubMed shows them; excerpts are short quotations from the abstracts.",
        "retrieved": date,
        "query": QUERY,
        "pubmed_count": len(ids),
        "left_out": dropped,
        "not_included": NOT_INCLUDED,
        "records": recs,
    }
    return doc, xml


def main():
    check = "--check" in sys.argv
    manifest_path = SNAP / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    old_entry = next((e for e in manifest["snapshots"] if e["id"] == SNAP_ID), None)
    date = datetime.date.today().isoformat()
    if "--date" in sys.argv:
        date = sys.argv[sys.argv.index("--date") + 1]
    elif check and old_entry:
        date = old_entry["retrieved"]
    doc, xml = build(date)
    path = SNAP / FILE
    text = json.dumps(doc, indent=1, ensure_ascii=False) + "\n"
    new = text.encode("utf-8")
    old = path.read_bytes() if path.exists() else b""
    print(f"{FILE}: sha256 {sha(new)}, bytes {len(new)}, records {len(doc['records'])} (PubMed returned {doc['pubmed_count']}; left out {doc['left_out']})")
    if check:
        print("unchanged" if old == new else "changed")
        sys.exit(0 if old == new else 1)
    path.write_bytes(new)
    dates = sorted(r["published"] for r in doc["records"])
    entry = {
        "id": SNAP_ID,
        "title": "Community Guide systematic reviews, economic reviews, and Task Force recommendation statements (journal papers indexed in PubMed)",
        "publisher": "Community Preventive Services Task Force and CDC Community Guide staff (papers in the American Journal of Preventive Medicine and other journals; records read through the National Library of Medicine's PubMed E-utilities)",
        "source_urls": SOURCE_URLS,
        "source_sha256": {"pubmed_efetch_response.xml": sha(xml)},
        "source_vintage": f"papers published {dates[0]} to {dates[-1]}",
        "retrieved": doc["retrieved"],
        "license": "Titles, dates, and MeSH headings are PubMed bibliographic data. Abstract excerpts are short quotations with attribution; abstracts can be copyrighted by the journal, so check the journal's terms before reproducing more. The Community Guide's own terms of use and the National Library of Medicine's PubMed data policy pages could not be read (blocked from the build network).",
        "derivation": "PubMed esearch for the Corporate Author 'Community Preventive Services Task Force' or 'Task Force on Community Preventive Services', then one efetch for every PubMed ID returned. The builder classifies each paper's kind from its title, copies rating words ('strong evidence', 'sufficient evidence', 'insufficient evidence', 'recommends', 'recommends against') only when the abstract contains them, and keeps one short excerpt each of the findings, the conclusion, and any implementation or barrier sentence. Nothing about a rating comes from memory. Builder: xtask/scripts/build_interventions_snapshots.py (Python 3 standard library).",
        "caveats": [
            "Publication dates are journal dates (year, and month when PubMed has it); the Community Guide's own posting date can differ.",
        ] + NOT_INCLUDED,
        "files": [{"path": FILE, "sha256": sha(new), "bytes": len(new), "rows": len(doc["records"])}],
    }
    snaps = [e for e in manifest["snapshots"] if e["id"] != SNAP_ID]
    snaps.append(entry)
    manifest["snapshots"] = snaps
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print("manifest updated")


if __name__ == "__main__":
    main()
