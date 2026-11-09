"""Build the Massachusetts youth survey snapshot from DPH's report tables.

usage: build_youth_survey.py OUT_CSV REPORT.docx:SURVEY:YEAR [...]

Each REPORT is a DOCX downloaded from https://www.mass.gov/lists/massachusetts-youth-health-survey-myhs
(e.g. 2025-massachusetts-youth-health-survey-report.docx:MYHS:2025 or
2023-massachusetts-yhs-and-yrbs-joint-report.docx:MYRBS:2023). Every table whose first cell begins
"Percentage of Massachusetts" is read: the indicator names are the header cells, each estimate row is
followed by its 95% confidence interval row, and rows with blank values start a new group (Grade,
Gender, Race/Ethnicity, ...). In the joint YHS/YRBS report, indicators marked ^ come from the MYHS
and are labeled so. "**" (not presented: fewer than 100 respondents or relative standard error over
30%) is kept as suppressed. Output columns: survey, year, level, topic, indicator, group_type, group,
percent, ci_low, ci_high, suppressed. To keep the file small the indicators are written to a
second file (OUT_CSV with "_indicators" before ".csv") and the estimates refer to them by number.
"""
import csv, re, sys, zipfile
import xml.etree.ElementTree as ET

W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"


def text(el):
    return re.sub(r"\s+", " ", "".join(t.text or "" for t in el.iter(W + "t"))).strip()


def parse(path, survey, year):
    root = ET.fromstring(zipfile.ZipFile(path).read("word/document.xml"))
    body = root.find(W + "body")
    topic = ""
    out = []
    for el in body:
        if el.tag == W + "p":
            t = text(el)
            m = re.match(r"^([A-Z][A-Z &/,'-]+?)\s+[–-]\s+MASSACHUSETTS", t)
            if m:
                topic = m.group(1).strip().capitalize().replace(" & ", " and ")
            continue
        if el.tag != W + "tbl":
            continue
        rows = [[text(tc) for tc in tr.findall(W + "tc")] for tr in el.iter(W + "tr")]
        if not rows or not rows[0] or not rows[0][0].lower().startswith("percentage of massachusetts"):
            continue
        level = "high" if "high school" in rows[0][0].lower() else "middle"
        indicators = rows[0][1:]
        group_type = "Overall"
        i = 1
        while i < len(rows):
            r = rows[i]
            label = r[0].strip()
            vals = r[1:]
            if label and all(v.strip() in ("", " ") for v in vals):
                group_type = label
                i += 1
                continue
            if not label or label.startswith("("):
                i += 1
                continue
            label = re.sub(r"\s*\(95% Confidence Interval\)", "", label).strip()
            # The joint report puts "23.6 (21.0 - 26.1)" in one cell; the MYHS reports put the
            # interval in the next row.
            combined = any(re.search(r"\d\s*[a-z]?\s*\(", v) for v in vals)
            nxt = rows[i + 1] if i + 1 < len(rows) else None
            ci = nxt[1:] if not combined and nxt and (not nxt[0].strip() or nxt[0].startswith("(")) else []
            for k, ind in enumerate(indicators):
                if not ind.strip():
                    continue
                v = vals[k].strip() if k < len(vals) else ""
                c = ci[k].strip() if k < len(ci) else ""
                if combined:
                    m = re.match(r"^([\d.]+)\s*[a-z]?\s*(\(.*\))?$", v)
                    if m:
                        v, c = m.group(1), (m.group(2) or "")
                src = survey
                name = re.sub(r"\s+", " ", ind).strip()
                if name.endswith("^") or name.startswith("^"):
                    src = "MYHS"
                    name = name.strip("^ ").strip()
                if survey == "MYRBS" and level == "middle":
                    src = "MYHS"  # the joint report's middle school results all come from the MYHS
                sup = v.startswith("**")
                v = re.sub(r"\.\.+", ".", v)  # one published typo: "91..0"
                pct = "" if sup or not re.match(r"^\d+(\.\d+)?$", v) else v
                m = re.match(r"^\(?\s*([\d.]+)\s*[-–]\s*([\d.]+)\s*\)?$", c)
                lo, hi = (m.group(1), m.group(2)) if m and pct else ("", "")
                gt = "Overall" if label.lower() == "overall" else group_type
                out.append([src, year, level, topic, name, gt, label, pct, lo, hi, "1" if sup else ""])
            i += 2 if ci else 1
    return out


out_path = sys.argv[1]
rows = []
for spec in sys.argv[2:]:
    path, survey, year = spec.rsplit(":", 2)
    got = parse(path, survey, year)
    print(spec, len(got))
    rows.extend(got)
seen = set()
uniq = []
for r in rows:
    key = (r[0], r[1], r[2], r[4], r[5], r[6])
    if key in seen:
        continue
    seen.add(key)
    uniq.append(r)
indicators = []
index = {}
for r in uniq:
    key = (r[0], r[1], r[2], r[3], r[4])
    if key not in index:
        index[key] = len(indicators) + 1
        indicators.append([index[key]] + list(key))
ind_path = out_path.replace(".csv", "_indicators.csv")
with open(ind_path, "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f, lineterminator="\n")
    w.writerow(["id", "survey", "year", "level", "topic", "indicator"])
    w.writerows(indicators)
with open(out_path, "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f, lineterminator="\n")
    w.writerow(["indicator_id", "group_type", "group", "percent", "ci_low", "ci_high", "suppressed"])
    for r in uniq:
        w.writerow([index[(r[0], r[1], r[2], r[3], r[4])]] + r[5:])
print("indicators", len(indicators), "rows", len(uniq))
