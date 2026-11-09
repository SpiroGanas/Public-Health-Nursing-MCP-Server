"""Build snapshots/ma_lead_screening_2024.csv from DPH's 2024 Annual Childhood Lead Poisoning
Surveillance Report (Appendix I, high-risk communities 2020-2024; Appendix II, screening and
blood lead levels by community, children 9 to 47 months, calendar year 2024).

usage: build_lead_snapshot.py REPORT_TXT REPORT_2023_DOCX TOWNS_CSV OUT_CSV

REPORT_TXT is `pdftotext -layout` output of
https://www.mass.gov/doc/2024-annual-childhood-lead-poisoning-surveillance-report/download
(PDF created 2026-08-05). DPH's 2024 PDF has no Appendix II page for the communities from
Monson to Northampton (24 communities, including New Bedford), so their rows come from the
2023 report (REPORT_2023_DOCX,
https://www.mass.gov/doc/2023-annual-childhood-lead-poisoning-surveillance-report/download)
and are marked data_year 2023. TOWNS_CSV is snapshots/ma_municipalities.csv (official names). Values are copied as printed (">99" where more children were
screened than the population estimate; "NA" where there are none); "NS" (not shown: 1-5 children where
fewer than 1,200 were screened) is kept as NS.
"""
import csv, re, sys

SRC, DOCX, TOWNS, OUT = sys.argv[1:5]
lines = open(SRC, encoding="utf-8").read().splitlines()

# Appendix I: high-risk communities.
hr = {}
in_app1 = False
for ln in lines:
    if "Appendix I: High-Risk Communities" in ln:
        in_app1 = True
        continue
    if in_app1:
        if ln.strip().startswith("ALL HIGH-RISK"):
            break
        m = re.match(r"\s{0,8}([A-Z][A-Z .'-]+?)\s{2,}(\d+)%\s+(\d+)\s+([\d.]+)\s+(\d+)\s+(\d+)\s+([\d.]+)\s*$", ln)
        if m:
            name = m.group(1).strip().title()
            hr[name] = {
                "hr_rank": len(hr) + 1,
                "hr_pct_screened_5yr": m.group(2),
                "hr_cases_5yr": m.group(3),
                "hr_incidence_per_1000": m.group(4),
                "hr_pct_pir_below_2": m.group(5),
                "hr_pct_pre1978": m.group(6),
                "hr_score": m.group(7),
            }

# Appendix II: one row per community.
rows = []
in_app2 = False
prefix = ""
tok = r"(>?\d+(?:\.\d+)?|NS|NA)"
pat = re.compile(r"^([A-Z][A-Za-z .'-]*?[a-z.A])\s+" + r"\s+".join([tok] * 16) + r"\s*$")
for ln in lines:
    if "Appendix II: Screening and Prevalence" in ln:
        in_app2 = True
        continue
    if "APPENDIX III" in ln:
        break
    if not in_app2:
        continue
    t = ln.rstrip()
    if t.startswith("Total for MA"):
        t = "Total for MA  " + re.sub(r"\s+", "  ", t[len("Total for MA"):].strip())
    m = pat.match(t)
    if m:
        name = m.group(1).strip()
        # pdftotext puts the first word of some names ("East", "Great", "North") on its own line.
        if prefix:
            name = prefix + " " + name
        rows.append([name] + [m.group(i) for i in range(2, 18)])
        prefix = ""
    elif re.fullmatch(r"[A-Z][a-z]+(?: [A-Z][a-z]+)?", t.strip()) and t.strip() not in ("Community", "Comments"):
        prefix = t.strip()
    else:
        prefix = ""

# 2023 rows (DOCX tables) for communities missing from the 2024 PDF.
import zipfile
x = zipfile.ZipFile(DOCX).read("word/document.xml").decode("utf-8")
rows23 = {}
for tbl in re.findall(r"<w:tbl>.*?</w:tbl>", x, re.S):
    for tr in re.findall(r"<w:tr[ >].*?</w:tr>", tbl, re.S):
        cells = ["".join(re.findall(r"<w:t[^>]*>([^<]*)</w:t>", tc)).strip()
                 for tc in re.findall(r"<w:tc>.*?</w:tc>", tr, re.S)]
        if len(cells) == 17 and re.fullmatch(r"\d+", cells[1] or "x"):
            rows23[cells[0].title()] = cells[1:]

ALIASES = {"Manchester": "Manchester-by-the-Sea"}
towns = [r["name"] for r in csv.DictReader(open(TOWNS, encoding="utf-8"))]
by_name = {}
for r in rows:
    by_name[ALIASES.get(r[0], r[0])] = (r[1:], "2024")
missing = []
for t in towns:
    if t not in by_name:
        src = rows23.get(t) or rows23.get(t.replace("-", " ").title())
        if src:
            by_name[t] = (src, "2023")
            missing.append(t)
total = by_name.pop("Total for MA")

header = ["town", "data_year", "population_9_47mo", "screened", "pct_screened",
          "bll_0_4_n", "bll_0_4_pct", "bll_5_9_n", "bll_5_9_pct", "bll_10_24_n", "bll_10_24_pct",
          "bll_25plus_n", "bll_25plus_pct", "est_confirmed_5plus_n", "est_confirmed_5plus_pct",
          "confirmed_10plus_n", "confirmed_10plus_pct", "pct_pre1978_housing",
          "high_risk_2024", "hr_rank", "hr_pct_screened_5yr", "hr_cases_5yr",
          "hr_incidence_per_1000", "hr_pct_pir_below_2", "hr_pct_pre1978", "hr_score"]
with open(OUT, "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f, lineterminator="\n")
    w.writerow(header)
    for t in towns + ["Total for MA"]:
        vals, year = total if t == "Total for MA" else by_name.get(t, ([""] * 16, ""))
        h = hr.get(t)
        extra = ["yes" if h else "no"] + ([h[k] for k in header[19:]] if h else [""] * 7)
        w.writerow([t, year] + list(vals) + extra)
print(len(by_name), "towns;", len(missing), "from 2023:", missing)
print("high-risk not matched:", [k for k in hr if k not in by_name])
print("no data:", [t for t in towns if t not in by_name])
