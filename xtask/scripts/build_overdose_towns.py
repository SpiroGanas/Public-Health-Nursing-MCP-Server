"""Build the DPH town-level opioid-related overdose death snapshot.

usage: build_overdose_towns.py LAYOUT_TEXT OUT_CSV

LAYOUT_TEXT is `pdftotext -layout` output of DPH's "Number of Opioid-Related Overdose Deaths,
All Intents by City/Town, 2015-2022" (posted June 2023):
https://www.mass.gov/doc/opioid-related-overdose-deaths-by-citytown-june-2023/download

Table 1 (city/town of residence, MA residents) and Table 2 (city/town of death occurrence) are
written as one CSV: basis,town,2015,...,2022. "Unknown", "Out of Massachusetts", and "Total" rows
are kept. The script checks that each table's town rows sum to its published total.
"""
import csv, re, sys

src, out = sys.argv[1:3]
YEARS = list(range(2015, 2023))
row_re = re.compile(r"^\s*([A-Z][A-Za-z .'\-]*?[a-z.])\s+((?:[\d,]+\s+){7}[\d,]+)\s*$")
table = None
rows = {"residence": [], "occurrence": []}
pending = ""  # first word of a town name wrapped onto two lines ("Mount" / "Washington ...")
for line in open(src, encoding="utf-8"):
    if re.fullmatch(r"\s*(Mount|North|South|East|West|New|Great|Manchester)\s*", line.rstrip("\n")):
        pending = line.strip() + " "
        continue
    if "Table 1." in line:
        table = "residence"
        continue
    if "Table 2." in line:
        table = "occurrence"
        continue
    m = row_re.match(line)
    if not m or table is None:
        pending = ""
        continue
    name = re.sub(r"\s+", " ", pending + m.group(1)).strip()
    pending = ""
    vals = [int(v.replace(",", "")) for v in m.group(2).split()]
    if name in ("Residence", "Occurrence"):
        continue
    rows[table].append([table, name] + vals)

with open(out, "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f, lineterminator="\n")
    w.writerow(["basis", "town"] + [str(y) for y in YEARS])
    for basis in ("residence", "occurrence"):
        data = rows[basis]
        towns = [r for r in data if r[1] not in ("Total",)]
        total = [r for r in data if r[1] == "Total"]
        assert len(total) == 1, (basis, len(total))
        sums = [sum(r[2 + i] for r in towns) for i in range(len(YEARS))]
        assert sums == total[0][2:], (basis, sums, total[0][2:])
        named = [r for r in towns if r[1] not in ("Unknown", "Out of Massachusetts")]
        print(basis, "towns", len(named), "rows", len(data))
        for r in data:
            w.writerow(r)
