#!/usr/bin/env python3
"""Build the Phase 19 law corpus snapshots (POL-02 search_public_health_law).

Usage (from the repository root, Python 3.9+; `pdftotext` from poppler for 105 CMR and the
`pypdf` package for 244 CMR; everything else is the standard library):

    python3 -I xtask/scripts/build_law_snapshots.py [--check] [--date YYYY-MM-DD] [--cache DIR]
    python3 -I xtask/scripts/build_law_snapshots.py --extended OUT_DIR [--date YYYY-MM-DD]
    python3 -I xtask/scripts/build_law_snapshots.py --selftest

What it reads (public pages and PDFs; mass.gov answers 403 to browser-like user agents, so the
requests use a plain curl user agent, as the server's own client does):

* M.G.L. chapter 111 (Public Health), malegislature.gov: the chapter page lists every section
  with its title (the index); the text of the sections in CORE_C111 is read from their pages.
* 105 CMR (Department of Public Health), mass.gov: the Table of Contents on any 105 CMR page
  lists every chapter; each chapter's page gives its date and a PDF. Every PDF is read for its
  table of contents (section numbers and titles); the sections of the chapters in FULL_105 are
  kept in full text.
* 244 CMR (Board of Registration in Nursing), mass.gov: the same, for all eight chapters, which
  are all kept in full text. The 244 CMR PDFs hold every page twice (two text layers with
  different spacing), so the script reads them with pypdf and keeps the second layer of each page.

Default mode writes snapshots/ma_law_mgl_c111.json, snapshots/ma_law_105_cmr.json and
snapshots/ma_law_244_cmr.json and updates their entries in snapshots/manifest.json. With
--check it writes nothing, rebuilds in memory, and exits 1 if the files on disk differ. With
--extended OUT_DIR it builds the larger optional corpus (all c. 111 sections and every chapter of
105 CMR in full text) in the same format, with a manifest of checksums, for installation as
/data/law_corpus/ or /cache/law_corpus/ (see docs/tools.md, `search_public_health_law`).

Nothing is filled from memory: every title and text is read from the downloaded page or PDF. The
bundled snapshot is capped at 1.0 MB per source (the script stops if a source is larger).
"""

import datetime
import hashlib
import html
import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
UA = "curl/8.5.0"
FORMAT = "phn-law-corpus"
FORMAT_VERSION = 1
CAP_BYTES = 1_000_000

MGL_BASE = "https://malegislature.gov"
MGL_C111 = MGL_BASE + "/Laws/GeneralLaws/PartI/TitleXVI/Chapter111"
MASS = "https://www.mass.gov"

# c. 111 sections kept in full text: the Department's and the boards of health's powers, disease
# control (isolation, quarantine, notification, records), nuisances and the sanitary code,
# immunization, newborn screening, and the confidentiality sections.
CORE_C111 = [
    "1", "2", "5", "6", "7", "8C", "17", "24B", "24I", "24M", "26", "27", "30", "31", "32",
    "70E", "70F", "70G", "72CC", "94A", "94B", "95", "96", "97", "98", "104", "105", "109",
    "110", "110A", "111", "111C", "112", "113", "119", "122", "122A", "123", "124", "125",
    "127A", "127B", "127C", "130", "131",
]

# 105 CMR chapters kept in full text (chapter numbers as they appear in section numbers).
FULL_105 = [
    172, 200, 201, 210, 220, 221, 222, 223, 270, 271, 300, 335, 360, 365, 400, 410, 460, 590,
    661, 665,
]

SOURCES = {
    "mgl_c111": {
        "file": "ma_law_mgl_c111.json",
        "id": "ma-law-mgl-c111",
        "label": "M.G.L. c. 111 (Public Health)",
        "kind": "statute",
    },
    "cmr_105": {
        "file": "ma_law_105_cmr.json",
        "id": "ma-law-105-cmr",
        "label": "105 CMR (Department of Public Health regulations)",
        "kind": "regulation",
    },
    "cmr_244": {
        "file": "ma_law_244_cmr.json",
        "id": "ma-law-244-cmr",
        "label": "244 CMR (Board of Registration in Nursing regulations)",
        "kind": "regulation",
    },
}


# ------------------------------------------------------------------ fetching


class Fetcher:
    def __init__(self, cache=None, pause=0.25):
        self.cache = Path(cache) if cache else None
        self.pause = pause
        if self.cache:
            self.cache.mkdir(parents=True, exist_ok=True)

    def get(self, url):
        key = hashlib.sha256(url.encode()).hexdigest()[:24]
        if self.cache and (self.cache / key).exists():
            return (self.cache / key).read_bytes()
        last = None
        for attempt in range(3):
            try:
                req = urllib.request.Request(url, headers={"User-Agent": UA})
                with urllib.request.urlopen(req, timeout=120) as r:
                    data = r.read()
                break
            except Exception as e:  # network errors are retried, then raised
                last = e
                time.sleep(2 * (attempt + 1))
        else:
            raise RuntimeError(f"could not fetch {url}: {last}")
        if self.cache:
            (self.cache / key).write_bytes(data)
        time.sleep(self.pause)
        return data

    def text(self, url):
        return self.get(url).decode("utf-8", errors="replace")


def sha(b):
    return hashlib.sha256(b).hexdigest()


def collapse(s):
    return re.sub(r"\s+", " ", s).strip()


def strip_tags(s):
    return collapse(html.unescape(re.sub(r"<[^>]+>", " ", s)))


# ------------------------------------------------------------------ M.G.L. c. 111


def parse_mgl_index(page):
    """(section, title, href) for every section on a chapter page."""
    out = []
    for m in re.finditer(
        r'<a href="(/Laws/GeneralLaws/[^"]+/Section[^"]+)">\s*<span class="section">(.*?)</span>\s*'
        r'<span class="sectionTitle">(.*?)</span>',
        page,
        flags=re.S,
    ):
        number = collapse(html.unescape(m.group(2)))
        number = re.sub(r"^Section\s+", "", number)
        title = collapse(html.unescape(m.group(3)))
        out.append((number, title, m.group(1)))
    return out


def parse_mgl_section(page):
    """The text of a section page, tags removed."""
    start = page.find("</h2>", page.find('id="skipTo"'))
    if start < 0:
        raise ValueError("section text not found")
    seg = page[start + 5 :]
    m = re.match(r"\s*(.*?)(?=\s*<(?:div|hr|nav|footer|script|form|section)\b)", seg, flags=re.S)
    if not m:
        raise ValueError("end of section text not found")
    return strip_tags(m.group(1))


def build_mgl(f, date, full_sections):
    page = f.text(MGL_C111)
    items = parse_mgl_index(page)
    if len(items) < 400:
        raise RuntimeError(f"c. 111 index has only {len(items)} sections")
    provisions = []
    repealed = 0
    for number, title, href in items:
        if title.lower().startswith("repealed"):
            repealed += 1
            continue
        url = MGL_BASE + urllib.parse.quote(href, safe="/%,")
        p = {
            "c": f"M.G.L. c. 111, § {number}",
            "n": number,
            "ch": "111",
            "t": title,
            "u": url,
        }
        if number in full_sections or full_sections == "all":
            text = parse_mgl_section(f.text(url))
            text = re.sub(r"^Section\s+\S+(?:\s+\d/\d)?\.\s*", "", text)
            p["x"] = text
        provisions.append(p)
    missing = [s for s in (full_sections if full_sections != "all" else []) if not any(p["n"] == s for p in provisions)]
    if missing:
        raise RuntimeError(f"requested c. 111 sections not in the index: {missing}")
    return {
        "source_id": "mgl_c111",
        "title": SOURCES["mgl_c111"]["label"],
        "kind": "statute",
        "retrieved": date,
        "index_url": MGL_C111,
        "currency": "Text as published by the Massachusetts Legislature on the retrieval date; the site gives no per-section effective date.",
        "repealed_sections_omitted": repealed,
        "chapters": [
            {
                "ch": "111",
                "title": "Public Health",
                "url": MGL_C111,
                "pdf_url": None,
                "date": None,
                "full_text": False,
            }
        ],
        "provisions": provisions,
    }


# ------------------------------------------------------------------ CMR (mass.gov)


def parse_cmr_toc(page, agency):
    """Chapters from the Table of Contents block of a mass.gov CMR page: (display, title)."""
    t = re.sub(r"<(script|style|title)[^>]*>.*?</\1>", "", page, flags=re.S)
    t = html.unescape(re.sub(r"<[^>]+>", "\n", t))
    out = []
    for line in (collapse(x) for x in t.split("\n")):
        m = re.match(rf"^{agency} CMR (\d+)\.(\d+): (.+)$", line)
        if m and (m.group(1), m.group(3)) not in [(a, b) for a, _, b in out]:
            out.append((m.group(1), f"{m.group(1)}.{m.group(2)}", m.group(3)))
    return [(display, title) for _, display, title in out]


def parse_cmr_chapter_page(page, agency):
    """(date ISO or None, pdf url or None) from a mass.gov regulation page."""
    d = re.search(r"Date:\s*</[^>]+>\s*(?:<[^>]+>\s*)*(\d\d)/(\d\d)/(\d{4})", page)
    date = f"{d.group(3)}-{d.group(1)}-{d.group(2)}" if d else None
    pdfs = [
        x
        for x in re.findall(r'href="((?:https://www\.mass\.gov)?/doc/[^"]*/download)"', page)
        if f"{agency}-cmr" in x.lower()
    ]
    pdf = None
    if pdfs:
        pdf = pdfs[0] if pdfs[0].startswith("http") else MASS + pdfs[0]
    return date, pdf


NOISE = [
    re.compile(r"^\s*(105|244) CMR:.*$"),
    re.compile(r"^\s*\(Mass\. Register.*\)\s*$"),
    re.compile(r"^\s*(\d+\.\d+|Section):\s*continued\s*$", re.I),
    re.compile(r"^\s*\d{1,2}/\d{1,2}/\d{2,4}\s+244 CMR\s*-\s*\d+\s*$"),
    re.compile(r"^\s*244 CMR\s*-\s*\d+\s*$"),
    re.compile(r"^\s*\d{1,2}/\d{1,2}/\d{2,4}\s*$"),
]
HEAD = re.compile(r"^(\d+\.\d+):[ \t]+(\S.*?)\s*$")


def parse_cmr_text(text):
    """Sections of a CMR chapter from its text: list of (number, title, body or None).

    The chapter opens with a table of contents (number: title, long titles wrapped onto indented
    lines); the body repeats each heading on its own line, followed by a blank line. Headings
    before the first repeated number are the table of contents.
    """
    raw = text.replace("\f", "\n").split("\n")
    lines = [ln.rstrip() for ln in raw if not any(p.match(ln) for p in NOISE)]
    heads = []  # (line index, number, title text)
    for i, ln in enumerate(lines):
        m = HEAD.match(ln)
        if m:
            title = m.group(2)
            j = i + 1
            while j < len(lines) and lines[j].startswith("  ") and lines[j].strip() and not HEAD.match(lines[j].strip()):
                # wrapped title lines are indented and directly follow the heading
                title += " " + lines[j].strip()
                j += 1
            heads.append((i, m.group(1), collapse(title), j))
    seen = set()
    toc_end = 0
    for k, (_, number, _, _) in enumerate(heads):
        if number in seen:
            toc_end = k
            break
        seen.add(number)
    else:
        toc_end = 0  # no table of contents: every heading is a body heading
    toc = heads[:toc_end]
    body = heads[toc_end:]
    titles = {}
    for _, number, title, _ in toc:
        titles.setdefault(number, title)
    sections = []
    order = []
    texts = {}
    for k, (i, number, title, j) in enumerate(body):
        end = body[k + 1][0] if k + 1 < len(body) else len(lines)
        chunk = collapse(" ".join(lines[j:end]))
        # the last section of a chapter carries the authority line: keep it (it is part of the text)
        if number not in texts:
            order.append(number)
            texts[number] = chunk
            titles.setdefault(number, title)
        else:
            texts[number] += " " + chunk
    for _, number, title, _ in toc:
        if number not in texts and number not in order:
            order.append(number)
            texts[number] = None
    for number in order:
        sections.append((number, titles.get(number, ""), texts[number]))
    # keep table-of-contents order for sections that have no body
    return sections


def pdf_text_poppler(data):
    with tempfile.TemporaryDirectory() as d:
        p = Path(d) / "x.pdf"
        p.write_bytes(data)
        out = subprocess.run(
            ["pdftotext", "-layout", str(p), "-"], capture_output=True, check=True
        )
    return out.stdout.decode("utf-8", errors="replace")


def pdf_text_pypdf(data):
    """Text of a 244 CMR PDF: each page holds the text twice. The first layer ends with the page
    footer ('m/d/yy 244 CMR - N'); the second layer, which follows it, has the cleaner line
    breaks, so keep that one (or the whole page if it is not duplicated)."""
    import io

    try:
        import pypdf
    except ImportError:
        raise SystemExit("244 CMR needs the pypdf package (pip install pypdf)")
    import logging

    logging.getLogger("pypdf").setLevel(logging.ERROR)
    reader = pypdf.PdfReader(io.BytesIO(data))
    pages = []
    footer = re.compile(r"^\s*\d{1,2}/\d{1,2}/\d{2,4}\s+244 CMR\s*-\s*\d+\s*$", re.M)
    for pg in reader.pages:
        t = pg.extract_text() or ""
        m = footer.search(t)
        after = t[m.end() :] if m else ""
        before = t[: m.start()] if m else t
        pages.append(after if len(after.strip()) > 0.3 * len(before.strip()) else before)
    return "\n".join(pages)


def clean_title(t):
    return collapse(t.replace("–", "-"))


def build_cmr(f, date, agency, listing_url, full_chapters, text_reader, slug_prefix):
    page = f.text(listing_url)
    toc = parse_cmr_toc(page, agency)
    if len(toc) < 5:
        raise RuntimeError(f"{agency} CMR table of contents has only {len(toc)} chapters")
    links = []
    for m in re.findall(rf'href="(/regulations/{agency}-CMR-[^"]+)"', page):
        if m not in links:
            links.append(m)
    by_ch = {}
    for href in links:
        m = re.match(rf"/regulations/{agency}-CMR-(\d+)-", href)
        if m:
            by_ch[int(m.group(1)) // 100] = href
    chapters = []
    provisions = []
    for display, title in toc:
        num = int(display.split(".")[0])
        href = by_ch.get(num)
        url = MASS + href if href else None
        cdate, pdf = None, None
        if url:
            try:
                cdate, pdf = parse_cmr_chapter_page(f.text(url), agency)
            except RuntimeError:
                pass
        full = full_chapters == "all" or num in full_chapters
        chapter = {
            "ch": str(num),
            "title": title,
            "number": f"{agency} CMR {display}",
            "url": url,
            "pdf_url": pdf,
            "date": cdate,
            "full_text": False,
        }
        if pdf:
            try:
                sections = parse_cmr_text(text_reader(f.get(pdf)))
            except Exception as e:
                print(f"warning: {agency} CMR {num}: {e}", file=sys.stderr)
                sections = []
            kept = 0
            for number, st, body in sections:
                if not number.startswith(f"{num}."):
                    continue
                p = {
                    "c": f"{agency} CMR {number}",
                    "n": number,
                    "ch": str(num),
                    "t": clean_title(st),
                }
                if full and body:
                    p["x"] = body
                    kept += 1
                provisions.append(p)
            chapter["full_text"] = bool(full and kept)
        chapters.append(chapter)
    label = SOURCES["cmr_105" if agency == "105" else "cmr_244"]["label"]
    return {
        "source_id": "cmr_105" if agency == "105" else "cmr_244",
        "title": label,
        "kind": "regulation",
        "retrieved": date,
        "index_url": listing_url,
        "currency": "Unofficial copies on mass.gov (Trial Court Law Libraries); the official text is published by the Secretary of the Commonwealth. Each chapter's date is the date shown on its mass.gov page.",
        "chapters": chapters,
        "provisions": provisions,
    }


# ------------------------------------------------------------------ output


def dump(corpus):
    """One provision per line, so a diff shows what changed."""
    head = {k: v for k, v in corpus.items() if k not in ("chapters", "provisions")}
    out = ["{"]
    out.append('"format": %s,' % json.dumps(FORMAT))
    out.append('"format_version": %d,' % FORMAT_VERSION)
    for k, v in head.items():
        out.append("%s: %s," % (json.dumps(k), json.dumps(v, ensure_ascii=False)))
    out.append('"chapters": [')
    out.append(",\n".join(json.dumps(c, ensure_ascii=False) for c in corpus["chapters"]))
    out.append("],")
    out.append('"provisions": [')
    out.append(",\n".join(json.dumps(p, ensure_ascii=False) for p in corpus["provisions"]))
    out.append("]")
    out.append("}")
    return "\n".join(out) + "\n"


def entry(key, corpus, text, source_bytes, caveats, derivation, license_text):
    s = SOURCES[key]
    full = sum(1 for p in corpus["provisions"] if p.get("x"))
    dates = sorted({c["date"] for c in corpus["chapters"] if c.get("date")})
    vintage = (
        f"retrieved {corpus['retrieved']}; chapter dates {dates[0]} to {dates[-1]}"
        if dates
        else f"retrieved {corpus['retrieved']}"
    )
    return {
        "id": s["id"],
        "title": s["label"] + " - section index and partial full text",
        "publisher": "Massachusetts Legislature"
        if key == "mgl_c111"
        else (
            "Massachusetts Department of Public Health (copies on mass.gov, Trial Court Law Libraries)"
            if key == "cmr_105"
            else "Massachusetts Board of Registration in Nursing (copies on mass.gov, Trial Court Law Libraries)"
        ),
        "source_urls": [corpus["index_url"]],
        "source_sha256": {"corpus-input": source_bytes},
        "source_vintage": vintage,
        "retrieved": corpus["retrieved"],
        "license": license_text,
        "derivation": derivation,
        "caveats": caveats,
        "files": [
            {
                "path": s["file"],
                "sha256": sha(text.encode()),
                "bytes": len(text.encode()),
                "rows": len(corpus["provisions"]),
            }
        ],
        "_full": full,
    }


def digest_of(corpus):
    return sha(json.dumps(corpus, sort_keys=True, ensure_ascii=False).encode())


LICENSE = (
    "Massachusetts statutes and regulations are public records published by the Commonwealth; "
    "mass.gov's and malegislature.gov's site policies were not read, so check them before "
    "republishing the text. The snapshot keeps section numbers, titles, and text for search; "
    "the tool returns excerpts of 25 words or fewer."
)


def build_all(f, date):
    mgl = build_mgl(f, date, CORE_C111)
    c105 = build_cmr(f, date, "105", MASS + "/regulations/105-CMR-30000-reportable-diseases-surveillance-and-isolation-and-quarantine-requirements", set(FULL_105), pdf_text_poppler, "105")
    c244 = build_cmr(f, date, "244", MASS + "/regulations/244-CMR-900-standards-of-conduct-for-nurses", "all", pdf_text_pypdf, "244")
    return {"mgl_c111": mgl, "cmr_105": c105, "cmr_244": c244}


def entries_for(corpora):
    texts = {k: dump(c) for k, c in corpora.items()}
    for k, t in texts.items():
        if len(t.encode()) > CAP_BYTES:
            raise SystemExit(f"{k}: {len(t.encode())} bytes exceeds the {CAP_BYTES}-byte cap")
    out = {}
    m = corpora["mgl_c111"]
    out["mgl_c111"] = entry(
        "mgl_c111", m, texts["mgl_c111"], digest_of(m),
        [
            "The index lists every section of c. 111 that is not repealed (the Legislature's chapter page); full text is kept only for the sections named in CORE_C111 of the builder script. For any other section the tool gives the number, title, and link, not the text.",
            "The Legislature's site gives no per-section effective date: check the section page for amendments made after the retrieval date.",
            "Repealed sections are not listed. Sections of other chapters (for example c. 111D, c. 94C, c. 112) are not included.",
        ],
        "xtask/scripts/build_law_snapshots.py reads the chapter page (section number, title, link) and, for the core sections, each section page, taking the text after the heading and before the page chrome; whitespace is collapsed.",
        LICENSE,
    )
    c = corpora["cmr_105"]
    out["cmr_105"] = entry(
        "cmr_105", c, texts["cmr_105"], digest_of(c),
        [
            "Every chapter on mass.gov's 105 CMR table of contents is listed; section numbers and titles come from each chapter's PDF. Full text is kept only for the chapters named in FULL_105 of the builder script. Other chapters are index-only.",
            "mass.gov's copies are unofficial; the official version is published by the Secretary of the Commonwealth. Each chapter's date is the date on its mass.gov page and may be earlier than a later amendment not yet posted.",
            "Section titles and text are from PDF text extraction; check the PDF for tables, forms, and appendices, which are not reliably extracted.",
        ],
        "xtask/scripts/build_law_snapshots.py reads the mass.gov table of contents, each chapter page (date and PDF link), and each PDF with pdftotext -layout. Headings before the first repeated section number are the table of contents; page headers, footers, and 'continued' lines are removed; whitespace is collapsed.",
        LICENSE,
    )
    n = corpora["cmr_244"]
    out["cmr_244"] = entry(
        "cmr_244", n, texts["cmr_244"], digest_of(n),
        [
            "All eight chapters of 244 CMR on mass.gov (3.00 to 10.00) are in full text.",
            "mass.gov's PDFs hold each page twice; the build keeps the first text layer. The Board marks the mass.gov copies as not official.",
            "Board advisory rulings are not regulations and are in the scope-of-practice guide (nursing_scope_of_practice_guide), not here.",
        ],
        "xtask/scripts/build_law_snapshots.py reads each chapter page (date and PDF link) and each PDF with pypdf, keeping the first text layer of each page, then splits sections the same way as for 105 CMR.",
        LICENSE,
    )
    return texts, out


def write_manifest(entries):
    path = SNAP / "manifest.json"
    manifest = json.loads(path.read_text())
    ids = {e["id"] for e in entries.values()}
    snaps = [e for e in manifest["snapshots"] if e["id"] not in ids]
    for e in entries.values():
        e = {k: v for k, v in e.items() if not k.startswith("_")}
        snaps.append(e)
    manifest["snapshots"] = snaps
    path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")


def build_extended(f, date, out_dir):
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    mgl = build_mgl(f, date, "all")
    c105 = build_cmr(f, date, "105", MASS + "/regulations/105-CMR-30000-reportable-diseases-surveillance-and-isolation-and-quarantine-requirements", "all", pdf_text_poppler, "105")
    c244 = build_cmr(f, date, "244", MASS + "/regulations/244-CMR-900-standards-of-conduct-for-nurses", "all", pdf_text_pypdf, "244")
    files = []
    for key, corpus in (("mgl_c111", mgl), ("cmr_105", c105), ("cmr_244", c244)):
        text = dump(corpus)
        name = SOURCES[key]["file"]
        (out / name).write_text(text)
        files.append(
            {
                "path": name,
                "source_id": key,
                "sha256": sha(text.encode()),
                "bytes": len(text.encode()),
                "provisions": len(corpus["provisions"]),
                "full_text": sum(1 for p in corpus["provisions"] if p.get("x")),
            }
        )
    manifest = {
        "format": FORMAT + "-manifest",
        "format_version": FORMAT_VERSION,
        "kind": "extended",
        "built": date,
        "files": files,
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"wrote the extended corpus to {out}: " + ", ".join(f"{x['path']} ({x['bytes']} bytes, {x['full_text']} full-text sections)" for x in files))


# ------------------------------------------------------------------ self test


SAMPLE_TOC = """                         105 CMR: DEPARTMENT OF PUBLIC HEALTH


105 CMR 999.000:       SAMPLE REGULATION

Section

999.001: Purpose
999.020: Definitions
999.135: A Very Long Title That Wraps Onto A Second Line Because It Does Not Fit On One
         Line Of The Table
999.200: Isolation

999.001: Purpose

                The purpose is to test.

999.020: Definitions

            Case. One who is ill.

(Mass. Register #1470 5/27/22)
                         105 CMR: DEPARTMENT OF PUBLIC HEALTH


999.020: continued

          Carrier. One who carries.

999.135: A Very Long Title That Wraps Onto A Second Line Because It Does Not Fit On One Line Of The Table

          Body of the long section.
"""


def selftest():
    secs = parse_cmr_text(SAMPLE_TOC)
    got = {n: (t, b) for n, t, b in secs}
    assert list(got) == ["999.001", "999.020", "999.135", "999.200"], list(got)
    assert got["999.001"] == ("Purpose", "The purpose is to test.")
    assert got["999.020"][1] == "Case. One who is ill. Carrier. One who carries.", got["999.020"]
    assert got["999.135"][0].endswith("Line Of The Table") and got["999.135"][1] == "Body of the long section."
    assert got["999.200"] == ("Isolation", None)
    page = '<a href="/Laws/GeneralLaws/PartI/TitleXVI/Chapter111/Section5"><span class="section">Section 5</span> <span class="sectionTitle">Powers &amp; duties</span></a>'
    assert parse_mgl_index(page) == [("5", "Powers & duties", "/Laws/GeneralLaws/PartI/TitleXVI/Chapter111/Section5")]
    sec = '<h2 id="skipTo">x</h2><p><p>Section 5. The department shall act.</p></p>\n<div class="x">'
    assert parse_mgl_section(sec) == "Section 5. The department shall act."
    toc = parse_cmr_toc("<ul><li>105 CMR 300.00: Reportable diseases</li><li>105 CMR 300.00: Reportable diseases</li></ul>", "105")
    assert toc == [("300.00", "Reportable diseases")], toc
    d, pdf = parse_cmr_chapter_page(
        '<div>Date:</div><div>05/27/2022</div><a href="https://www.mass.gov/doc/105-cmr-300-x/download">', "105"
    )
    assert d == "2022-05-27" and pdf.endswith("/105-cmr-300-x/download"), (d, pdf)
    print("selftest ok")


# ------------------------------------------------------------------ main


def main():
    argv = sys.argv[1:]
    if "--selftest" in argv:
        selftest()
        return
    date = datetime.date.today().isoformat()
    if "--date" in argv:
        date = argv[argv.index("--date") + 1]
    cache = argv[argv.index("--cache") + 1] if "--cache" in argv else None
    if not shutil.which("pdftotext"):
        raise SystemExit("pdftotext (poppler-utils) is needed to read the 105 CMR PDFs")
    f = Fetcher(cache)
    if "--extended" in argv:
        build_extended(f, date, argv[argv.index("--extended") + 1])
        return
    check = "--check" in argv
    manifest = json.loads((SNAP / "manifest.json").read_text())
    if check:
        for e in manifest["snapshots"]:
            if e["id"] == SOURCES["mgl_c111"]["id"]:
                date = e["retrieved"]
    corpora = build_all(f, date)
    texts, entries = entries_for(corpora)
    if check:
        ok = True
        for key, text in texts.items():
            name = SOURCES[key]["file"]
            disk = (SNAP / name).read_text()
            if disk != text:
                print(f"{name}: differs from a fresh build", file=sys.stderr)
                ok = False
        sys.exit(0 if ok else 1)
    for key, text in texts.items():
        (SNAP / SOURCES[key]["file"]).write_text(text)
    write_manifest(entries)
    for key, c in corpora.items():
        full = sum(1 for p in c["provisions"] if p.get("x"))
        print(f"wrote {SOURCES[key]['file']}: {len(c['provisions'])} sections, {full} in full text, {len(texts[key])} bytes")


if __name__ == "__main__":
    main()
