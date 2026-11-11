#!/usr/bin/env python3
"""Build the Phase 14 chemicals and food snapshots (ENV-08, CHR-06).

Usage (from the repository root, Python 3.9+, standard library only):

    python3 -I xtask/scripts/build_chemfood_snapshots.py WORK_DIR [--check] [--only NAME,...]

WORK_DIR is an empty scratch folder for downloads (never the repository). The
script downloads the sources (politely: one request at a time with a short
pause), writes the snapshot files to snapshots/, and prints the manifest
fields (file checksums, bytes, rows, source checksums) to paste into
snapshots/manifest.json. With --check it writes nothing and reports whether
the files would change. --only limits the run to some of: npg, mrl, toxfaqs,
food.

Snapshots:

* niosh_npg.csv        NIOSH Pocket Guide to Chemical Hazards: one row per
                       entry (npgd0001-npgd0685): name, synonyms, CAS, DOT
                       number and ERG guide, conversion, IDLH, NIOSH REL,
                       OSHA PEL, physical description, exposure routes,
                       symptoms, target organs, cancer site, first aid, and
                       the page's "last reviewed" date. Respirator, protective
                       clothing, measurement, and property cards are left out.
* atsdr_mrls.csv       ATSDR Minimal Risk Levels (the whole table at
                       wwwn.cdc.gov/TSP/MRLS/mrlsListing.aspx): substance,
                       route, duration, MRL, total factors, endpoint,
                       draft/final, cover date, CAS.
* atsdr_toxfaqs.csv    ATSDR ToxFAQs index (letters A-Z of the ToxFAQs
                       landing page): title, CAS numbers (from each ToxFAQs
                       page), web page and PDF links. Text is not copied.
* usda_food_access_ma.csv
                       USDA ERS Food Access Research Atlas for Massachusetts:
                       the 2025 SNAP-authorized Retailer Access Map (SRAM,
                       2020 census tracts) general tract characteristics and
                       the straight-line (sd_) and driving (dd_) distance
                       measures, plus the 2019 Large Retailer Access Map
                       (LRAM, formerly the Food Access Research Atlas, 2010
                       tracts) low-income/low-access flags carried to 2020
                       tracts with the Census 2020-2010 tract relationship
                       file: lram_*_share is the share of the 2020 tract's
                       land area that lay in a 2010 tract with the flag.
"""

import csv
import hashlib
import html
import http.cookiejar
import io
import json
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
UA = "phn-mcp-server snapshot builder (+https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server)"
PAUSE = 0.4  # seconds between requests to the same host

NPG_BASE = "https://www.cdc.gov/niosh/npg/"
MRL_URL = "https://wwwn.cdc.gov/TSP/MRLS/mrlsListing.aspx"
TOX_LANDING = "https://wwwn.cdc.gov/TSP/ToxFAQs/ToxFAQsLanding.aspx"
TOX_DETAILS = "https://wwwn.cdc.gov/TSP/ToxFAQs/"
SRAM_ZIP = ("https://www.ers.usda.gov/media/29395/"
            "2025-snap-authorized-retailer-access-map-sram-data.zip")
LRAM_ZIP = ("https://www.ers.usda.gov/media/5627/2019-large-retailer-access-map-lram-formerly-"
            "known-as-the-food-access-research-atlas-fara-data.zip")
REL_URL = ("https://www2.census.gov/geo/docs/maps-data/data/rel2020/tract/"
           "tab20_tract20_tract10_st25.txt")

_cookies = http.cookiejar.CookieJar()
_opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(_cookies))


def fetch(url, data=None, tries=3):
    req = urllib.request.Request(url, data=data, headers={"User-Agent": UA})
    for attempt in range(tries):
        try:
            with _opener.open(req, timeout=180) as r:
                body = r.read()
            time.sleep(PAUSE)
            return body
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return None
            if attempt == tries - 1:
                raise
        except (urllib.error.URLError, TimeoutError):
            if attempt == tries - 1:
                raise
        time.sleep(3 * (attempt + 1))
    return None


def cached(work, name, url):
    """Download once into WORK_DIR (re-runs reuse the file)."""
    p = work / name
    if p.exists() and p.stat().st_size > 0:
        return p.read_bytes()
    b = fetch(url)
    if b is None:
        raise SystemExit(f"404: {url}")
    p.write_bytes(b)
    return b


def sha(b):
    return hashlib.sha256(b).hexdigest()


def text_of(fragment):
    s = re.sub(r"<br\s*/?>", "; ", fragment, flags=re.I)
    # Inline markup (italic n- prefixes, sub/superscripts) joins without a space.
    s = re.sub(r"</?(?:em|i|sup|sub|span)\b[^>]*>", "", s, flags=re.I)
    s = re.sub(r"<[^>]+>", " ", s)
    s = html.unescape(s).replace(" ", " ")
    s = re.sub(r"\s+", " ", s).strip()
    s = re.sub(r"\s*;\s*(;\s*)+", "; ", s)
    s = re.sub(r"\s+([,.;:)])", r"\1", s).strip(" ;")
    return s


# ---------------------------------------------------------------- NIOSH NPG

NPG_FIELDS = ["id", "name", "synonyms", "cas", "dot", "conversion", "idlh", "rel", "pel",
              "physical_description", "exposure_routes", "symptoms", "target_organs",
              "cancer_site", "first_aid", "url", "last_reviewed"]


def npg_ids(work):
    ids = set()
    pages = [f"npgsyn-{c}.html" for c in "abcdefghijklmnopqrstuvwxyz"] + ["npgdcas.html"]
    for p in pages:
        b = cached(work / "npg", p, NPG_BASE + p)
        ids.update(re.findall(r"npgd(\d{4})\.html", b.decode("utf-8", "replace")))
    return sorted(ids)


def parse_npg(page, ident):
    t = page.decode("utf-8", "replace")
    main = t[t.find('<main'):t.find('</main>')]
    name = text_of(re.search(r"<h1[^>]*>(.*?)</h1>", main, re.S).group(1))
    cards = {}
    parts = re.split(r"""<div [Cc]lass=['"]card-title h6['"]>""", main)
    for part in parts[1:]:
        title_html, _, body = part.partition("</div>")
        title = text_of(title_html)
        cards.setdefault(title, body)

    def card(title):
        body = cards.get(title)
        if body is None:
            return ""
        return body

    def plain(title):
        return text_of(card(title))

    limits = card("Exposure Limits")
    rel = pel = ""
    for m in re.finditer(r"<h6>(.*?)</h6>(.*?)(?=<h6>|$)", limits, re.S):
        label = text_of(m.group(1))
        val = text_of(m.group(2))
        if label.startswith("NIOSH REL"):
            rel = val
        elif label.startswith("OSHA PEL"):
            pel = val
    idlh = plain("IDLH")
    idlh = re.sub(r";?\s*See:.*$", "", idlh).strip(" ;")
    first_aid = plain("First Aid")
    first_aid = re.sub(r"^\(See procedures\)\s*;?\s*", "", first_aid)
    first_aid = re.sub(r"\s*;\s*", "; ", first_aid).strip(" ;")
    first_aid = re.sub(r"(Eye|Skin|Breathing|Swallow):\s*", r"\1: ", first_aid)
    syn = plain("Synonyms & Trade Names")
    dot = plain("DOT ID & Guide")
    reviewed = re.search(r'last-reviewed-date">([^<]*)<', t)
    return {
        "id": f"npgd{ident}",
        "name": name,
        "synonyms": syn,
        "cas": plain("CAS No."),
        "dot": dot,
        "conversion": plain("Conversion"),
        "idlh": idlh,
        "rel": rel,
        "pel": pel,
        "physical_description": plain("Physical Description"),
        "exposure_routes": plain("Exposure Routes"),
        "symptoms": plain("Symptoms"),
        "target_organs": plain("Target Organs"),
        "cancer_site": plain("Cancer Site"),
        "first_aid": first_aid,
        "url": f"{NPG_BASE}npgd{ident}.html",
        "last_reviewed": reviewed.group(1).strip() if reviewed else "",
    }


def npg(work):
    (work / "npg").mkdir(parents=True, exist_ok=True)
    ids = npg_ids(work)
    rows = []
    src = {}
    for i, ident in enumerate(ids):
        b = cached(work / "npg", f"npgd{ident}.html", f"{NPG_BASE}npgd{ident}.html")
        src[f"npgd{ident}.html"] = sha(b)
        rows.append(parse_npg(b, ident))
        if (i + 1) % 100 == 0:
            print(f"  NPG {i + 1}/{len(ids)}", file=sys.stderr)
    out = io.StringIO()
    w = csv.DictWriter(out, fieldnames=NPG_FIELDS, lineterminator="\n")
    w.writeheader()
    for r in rows:
        w.writerow(r)
    # Too many pages to list each checksum in the manifest: one digest of all.
    digest = sha("".join(f"{k}:{v}\n" for k, v in sorted(src.items())).encode())
    return out.getvalue(), {"npgd*.html (sha256 of the sorted per-page digests)": digest}, len(rows)


# ---------------------------------------------------------------- ATSDR MRLs

MRL_FIELDS = ["substance", "toxid", "route", "duration", "mrl", "factors", "endpoint", "status",
              "cover_date", "cas"]


def mrl(work):
    b = cached(work, "mrlsListing.html", MRL_URL)
    t = b.decode("utf-8", "replace")
    table = t[t.find('id="ContentPlaceHolder1_mrlTable"'):]
    table = table[:table.find("</table>")]
    title = re.search(r'lblMRLTableTitle">([^<]*)<', t).group(1).strip()
    rows = []
    substance = toxid = None
    for tr in re.findall(r"<tr[^>]*>(.*?)</tr>", table, re.S):
        head = re.search(r'class="bg-secondary"[^>]*>(.*?)</td>', tr, re.S)
        if head:
            substance = text_of(head.group(1))
            m = re.search(r"toxid=(\d+)", head.group(1))
            toxid = m.group(1) if m else ""
            continue
        cells = [text_of(c) for c in re.findall(r"<td[^>]*>(.*?)</td>", tr, re.S)]
        if len(cells) != 8 or not substance:
            continue
        if not cells[0] and not cells[2] and rows and rows[-1]["substance"] == substance:
            # A continuation row: another uncertainty factor and endpoint for the MRL above
            # (trichloroethylene's MRLs rest on more than one endpoint).
            rows[-1]["factors"] += "; " + cells[3]
            rows[-1]["endpoint"] += "; " + cells[4]
            continue
        if cells[0] == "Inh":
            cells[0] = "Inh."
        rows.append(dict(zip(MRL_FIELDS, [substance, toxid] + cells)))
    total = re.search(r"Total number of MRLs:\s*(\d+)", text_of(t))
    # The page's own tally counts routes spelled exactly "Inh." and "Oral"; one row
    # spells the route "Inh", so the table can have one more row than the tally.
    if total and abs(int(total.group(1)) - len(rows)) > 1:
        raise SystemExit(f"MRL table has {len(rows)} rows; the page says {total.group(1)}")
    out = io.StringIO()
    w = csv.DictWriter(out, fieldnames=MRL_FIELDS, lineterminator="\n")
    w.writeheader()
    for r in rows:
        w.writerow(r)
    return out.getvalue(), {"mrlsListing.aspx": sha(b)}, len(rows), title


# ---------------------------------------------------------------- ATSDR ToxFAQs

TOX_FIELDS = ["title", "faqid", "toxid", "cas", "url", "pdf_url"]


def hidden_fields(t):
    return {m.group(1): html.unescape(m.group(2)) for m in re.finditer(
        r'<input type="hidden" name="([^"]+)" id="[^"]*" value="([^"]*)"', t)}


def toxfaqs(work):
    (work / "toxfaqs").mkdir(parents=True, exist_ok=True)
    landing = fetch(TOX_LANDING).decode("utf-8", "replace")
    buttons = re.findall(
        r'<input type="submit" name="(ctl00\$ContentPlaceHolder1\$\w+)" value="([A-Z])"', landing)
    if len(buttons) < 20:
        raise SystemExit(f"ToxFAQs: only {len(buttons)} letter buttons found")
    entries = {}
    pages = {}
    for name, letter in buttons:
        form = hidden_fields(landing)
        form[name] = letter
        page = fetch(TOX_LANDING, data=urllib.parse.urlencode(form).encode())
        pages[letter] = page
        (work / "toxfaqs" / f"letter_{letter}.html").write_bytes(page)
        for href, label in re.findall(
                r'href="(ToxFAQsDetails\.aspx\?faqid=\d+&amp;toxid=\d+)"[^>]*>(.*?)</a>',
                page.decode("utf-8", "replace"), re.S):
            href = html.unescape(href)
            faqid = re.search(r"faqid=(\d+)", href).group(1)
            toxid = re.search(r"toxid=(\d+)", href).group(1)
            entries.setdefault(faqid, (text_of(label), toxid))
    rows = []
    src = {}
    for faqid, (title, toxid) in sorted(entries.items(), key=lambda e: e[1][0].lower()):
        url = f"{TOX_DETAILS}ToxFAQsDetails.aspx?faqid={faqid}&toxid={toxid}"
        b = cached(work / "toxfaqs", f"faq{faqid}.html", url)
        src[f"faq{faqid}"] = sha(b)
        t = b.decode("utf-8", "replace")
        plain = text_of(t[t.find("<main") if "<main" in t else 0:])
        # "CAS#: 71-43-2" or "CAS#: Mirex 2385-85-5; Chlordecone 143-50-5", before the PDF
        # link. Some ToxFAQs (classes such as dioxins) list no CAS number.
        m = re.search(r"CAS\s*#?:?(.*?)PDF Version", plain)
        cas = ";".join(dict.fromkeys(re.findall(r"\b\d{2,7}-\d{2}-\d\b", m.group(1)))) if m else ""
        pdf = re.search(r'href="(https?://www\.atsdr\.cdc\.gov/toxfaqs/[^"]+\.pdf)"', t)
        rows.append({"title": title, "faqid": faqid, "toxid": toxid, "cas": cas, "url": url,
                     "pdf_url": pdf.group(1) if pdf else ""})
    out = io.StringIO()
    w = csv.DictWriter(out, fieldnames=TOX_FIELDS, lineterminator="\n")
    w.writeheader()
    for r in rows:
        w.writerow(r)
    digest = sha("".join(f"{k}:{v}\n" for k, v in sorted(src.items())).encode())
    return out.getvalue(), {"ToxFAQsDetails pages (sha256 of the sorted per-page digests)": digest}, len(rows)


# ---------------------------------------------------------------- USDA Food Access

GENERAL = [("urban", "Urban"), ("pop2020", "POP2020"), ("ohu2020", "OHU2020"),
           ("group_quarters_flag", "GroupQuartersFlag"), ("low_income", "LowIncomeTracts"),
           ("poverty_rate", "PovertyRate"), ("median_family_income", "MedianFamilyIncome"),
           ("low_income_pop", "TractLOWI"), ("kids", "TractKids"), ("seniors", "TractSeniors"),
           ("hh_no_vehicle", "TractHUNV"), ("hh_snap", "TractSNAP")]
MEASURE = [("lila_1_10", "LILATracts_1And10"), ("lila_half_10", "LILATracts_halfAnd10"),
           ("lila_1_20", "LILATracts_1And20"), ("lila_vehicle", "LILATracts_Vehicle"),
           ("hunv_flag", "HUNVFlag"), ("la_1_10", "LA1and10"), ("la_half_10", "LAhalfand10"),
           ("la_1_20", "LA1and20"), ("lapop_1_10", "LAPOP1_10"), ("lapop_half_10", "LAPOP05_10"),
           ("lapop_1_20", "LAPOP1_20"), ("lalowi_1_10", "LALOWI1_10"),
           ("lalowi_half_10", "LALOWI05_10"), ("lalowi_1_20", "LALOWI1_20"),
           ("lahunv_half", "lahunvhalf"), ("lahunv_1", "lahunv1"),
           ("lakids_1", "lakids1"), ("laseniors_1", "laseniors1")]
LRAM = [("lram_lila_1_10_share", "LILATracts_1And10"),
        ("lram_lila_half_10_share", "LILATracts_halfAnd10"),
        ("lram_lila_1_20_share", "LILATracts_1And20"),
        ("lram_lila_vehicle_share", "LILATracts_Vehicle")]


def ma_rows(zf, member, key):
    with zf.open(member) as f:
        raw = f.read()
    for enc in ("utf-8-sig", "cp1252"):
        try:
            text = raw.decode(enc)
            break
        except UnicodeDecodeError:
            continue
    rows = {}
    for r in csv.DictReader(io.StringIO(text)):
        if r.get("State") == "Massachusetts":
            rows[r[key]] = r
    return rows


def clean_num(v):
    v = (v or "").strip()
    if v == "":
        return ""
    try:
        x = float(v)
    except ValueError:
        return ""
    return str(int(x)) if x == int(x) else f"{x:.4g}"


def food(work):
    sram_b = cached(work, "sram.zip", SRAM_ZIP)
    lram_b = cached(work, "lram.zip", LRAM_ZIP)
    rel_b = cached(work, "tab20_tract20_tract10_st25.txt", REL_URL)
    sz = zipfile.ZipFile(io.BytesIO(sram_b))
    gen = ma_rows(sz, "SRAM General Tract Characteristics Data.csv", "CensusTract20")
    sd = ma_rows(sz, "SRAM Straight Line Distance Data.csv", "CensusTract20")
    dd = ma_rows(sz, "SRAM Driving Distance Data.csv", "CensusTract20")
    lz = zipfile.ZipFile(io.BytesIO(lram_b))
    lram = ma_rows(lz, "Food Access Research Atlas.csv", "CensusTract")
    # 2020 tract -> [(2010 tract, land area of the part)], and 2020 land areas.
    parts = {}
    land20 = {}
    rel_text = rel_b.decode("utf-8-sig")
    for r in csv.DictReader(io.StringIO(rel_text), delimiter="|"):
        t20, t10 = r["GEOID_TRACT_20"], r["GEOID_TRACT_10"]
        land20[t20] = int(r["AREALAND_TRACT_20"] or 0)
        parts.setdefault(t20, []).append((t10, int(r["AREALAND_PART"] or 0)))
    header = (["geoid"] + [k for k, _ in GENERAL] + [f"sd_{k}" for k, _ in MEASURE]
              + [f"dd_{k}" for k, _ in MEASURE] + [k for k, _ in LRAM])
    out = io.StringIO()
    w = csv.writer(out, lineterminator="\n")
    w.writerow(header)
    missing_lram = set()
    for geoid in sorted(gen):
        g, s, d = gen[geoid], sd.get(geoid, {}), dd.get(geoid, {})
        row = [geoid] + [clean_num(g.get(c)) for _, c in GENERAL]
        row += [clean_num(s.get(f"SD_SRAM_{c}")) for _, c in MEASURE]
        row += [clean_num(d.get(f"DD_SRAM_{c}")) for _, c in MEASURE]
        area = land20.get(geoid, 0)
        for _, c in LRAM:
            if area <= 0 or geoid not in parts:
                row.append("")
                continue
            share = 0.0
            known = 0
            for t10, part in parts[geoid]:
                src = lram.get(t10)
                if src is None:
                    if part > 0:
                        missing_lram.add(t10)
                    continue
                known += part
                if src.get(c) == "1":
                    share += part
            row.append(f"{share / area:.3f}" if known > 0 else "")
        w.writerow(row)
    if missing_lram:
        print(f"  note: {len(missing_lram)} 2010 tracts with land are not in the LRAM file",
              file=sys.stderr)
    src = {"2025-snap-authorized-retailer-access-map-sram-data.zip": sha(sram_b),
           "2019-large-retailer-access-map-lram-...-fara-data.zip": sha(lram_b),
           "tab20_tract20_tract10_st25.txt": sha(rel_b)}
    return out.getvalue(), src, len(gen)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if not args:
        raise SystemExit(__doc__)
    work = Path(args[0]).resolve()
    if ROOT in work.parents or work == ROOT:
        raise SystemExit("WORK_DIR must be outside the repository")
    work.mkdir(parents=True, exist_ok=True)
    check = "--check" in sys.argv
    only = None
    if "--only" in sys.argv:
        only = set(sys.argv[sys.argv.index("--only") + 1].split(","))
    results = {}
    if only is None or "npg" in only:
        text, src, rows = npg(work)
        results["niosh_npg.csv"] = (text, src, rows, "")
    if only is None or "mrl" in only:
        text, src, rows, title = mrl(work)
        results["atsdr_mrls.csv"] = (text, src, rows, title)
    if only is None or "toxfaqs" in only:
        text, src, rows = toxfaqs(work)
        results["atsdr_toxfaqs.csv"] = (text, src, rows, "")
    if only is None or "food" in only:
        text, src, rows = food(work)
        results["usda_food_access_ma.csv"] = (text, src, rows, "")
    for name, (text, src, rows, note) in results.items():
        data = text.encode("utf-8")
        path = SNAP / name
        changed = not path.exists() or path.read_bytes() != data
        if not check:
            path.write_bytes(data)
        print(json.dumps({"path": name, "sha256": sha(data), "bytes": len(data), "rows": rows,
                          "source_sha256": src, "note": note, "changed": changed}, indent=1))


if __name__ == "__main__":
    main()
