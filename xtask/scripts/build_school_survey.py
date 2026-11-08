"""Build snapshots/ma_school_immunization_survey.csv from DPH's school survey XLSX files.

usage: build_school_survey.py OUT.csv K.xlsx G7.xlsx K3YR.xlsx
Rates are converted from fractions (0.91) to whole percents (91). DPH's markers are kept as
status codes: '†' -> suppressed_lt30, '*' -> no_response, '¶' -> discrepancy, 'N/A' (survey
returned with zero students) -> no_students.
"""
import csv, sys, openpyxl

OUT, K, G7, K3 = sys.argv[1:5]
MEASURES = {
    "5\nDTAP": "dtap", "4\nPOLIO": "polio", "2\nMMR": "mmr", "3\nHEP B": "hepb", "2\nVAR": "var",
    "1\nTDAP": "tdap", "1\nMEN": "men", "SERIES": "series", "EXEMPTION": "exemption",
    "UN-\nIMMUNIZED": "unimmunized", "NO\nRECORD": "no_record", "NON-\nCOMPLIANCE": "noncompliance",
}
COLS = ["dtap", "polio", "mmr", "hepb", "var", "tdap", "men", "series", "exemption",
        "unimmunized", "no_record", "noncompliance"]
MARK = {"†": "suppressed_lt30", "*": "no_response", "¶": "discrepancy", "N/A": "no_students"}

rows = []
for path, grade, period in [(K, "K", "2025-26"), (G7, "7", "2025-26"), (K3, "K", "2023-26 combined")]:
    wb = openpyxl.load_workbook(path, read_only=True, data_only=True)
    ws = [w for w in wb.worksheets if w.title != "Notes"][0]
    it = ws.iter_rows(values_only=True)
    header = [str(h).strip() if h is not None else "" for h in next(it)]
    idx = {}
    for i, h in enumerate(header):
        key = MEASURES.get(h.replace(" \n", "\n").strip())
        if key is None:
            key = MEASURES.get(h.strip().replace("MMR ", "MMR"))
        if key:
            idx[key] = i
    for r in it:
        if not r or r[0] is None:
            continue
        name = str(r[0]).strip()
        rec = {"grade": grade, "period": period, "school": name, "school_type": str(r[1] or "").strip(),
               "municipality": str(r[2] or "").strip(), "county": str(r[3] or "").strip(), "status": "reported"}
        for c in COLS:
            v = r[idx[c]] if c in idx else None
            if c not in idx:
                rec[c] = ""
            elif isinstance(v, (int, float)):
                rec[c] = str(round(v * 100))
            else:
                s = str(v or "").strip()
                if s in MARK:
                    rec["status"] = MARK[s]
                rec[c] = ""
        rows.append(rec)
fields = ["grade", "period", "school", "school_type", "municipality", "county", "status"] + COLS
with open(OUT, "w", newline="", encoding="utf-8") as f:
    w = csv.DictWriter(f, fieldnames=fields, lineterminator="\n")
    w.writeheader()
    w.writerows(rows)
print(len(rows), "rows")
