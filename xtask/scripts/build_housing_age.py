"""Build snapshots/ma_acs5_housing_age.csv for map_lead_risk (MCH-05): housing units built
before 1950 and before 1980 (ACS table B25034, Year Structure Built) for Massachusetts, its
counties, its 351 cities and towns, and the census tracts in snapshots/ma_acs5_indicators.csv.

usage: build_housing_age.py B25034_DAT INDICATORS_CSV OUT_CSV

B25034_DAT is https://www2.census.gov/programs-surveys/acs/summary_file/2024/table-based-SF/
data/5YRData/acsdt5y2024-b25034.dat (ACS 2020-2024 5-year table-based summary file; no API
key needed). Cells: E001 total; E010 built 1940-1949; E011 1939 or earlier; E007-E009
1950-1979. Sums use sqrt(sum of squared MOEs), counting only the largest MOE among zero
estimates (the Census Bureau's approximation, as in xtask/src/public_data.rs); percentages
use the derived-proportion formula (ratio formula when the radicand is negative). ACS year
bins do not split at 1978, so "pre-1980" is the closest available to the Lead Law's 1978.
"""
import csv, math, sys

DAT, IND, OUT = sys.argv[1:4]

wanted = {}
with open(IND, newline="", encoding="utf-8") as f:
    for r in csv.DictReader(f):
        if r["level"] in ("state", "county", "town", "tract"):
            wanted[r["geoid"]] = (r["level"], r["name"])

PREFIX = {"0400000US": "", "0500000US": "", "0600000US": "", "1400000US": ""}


def moe_sum(pairs):
    zeros = [m for e, m in pairs if e == 0]
    nz = [m for e, m in pairs if e != 0]
    if zeros:
        nz.append(max(zeros))
    return math.sqrt(sum(m * m for m in nz))


def pct(num, num_moe, den, den_moe):
    if den <= 0:
        return "", ""
    p = num / den
    rad = num_moe ** 2 - p * p * den_moe ** 2
    if rad < 0:
        rad = num_moe ** 2 + p * p * den_moe ** 2
    return f"{100 * p:.1f}", f"{100 * math.sqrt(rad) / den:.1f}"


rows = []
with open(DAT, encoding="utf-8") as f:
    header = f.readline().rstrip("\n").split("|")
    idx = {h: i for i, h in enumerate(header)}
    for line in f:
        parts = line.rstrip("\n").split("|")
        gid = parts[0]
        if not gid[:9] in PREFIX or not gid[9:].startswith("25"):
            continue
        geoid = gid[9:]
        if geoid not in wanted:
            continue
        e = lambda k: float(parts[idx[f"B25034_E{k:03d}"]] or 0)
        m = lambda k: float(parts[idx[f"B25034_M{k:03d}"]] or 0)
        total, total_m = e(1), m(1)
        p50 = [(e(k), m(k)) for k in (10, 11)]
        p80 = [(e(k), m(k)) for k in (7, 8, 9, 10, 11)]
        n50, n50m = sum(x for x, _ in p50), moe_sum(p50)
        n80, n80m = sum(x for x, _ in p80), moe_sum(p80)
        a, am = pct(n50, n50m, total, total_m)
        b, bm = pct(n80, n80m, total, total_m)
        level, name = wanted[geoid]
        rows.append([geoid, level, name, int(total), int(round(total_m)), int(n50), int(round(n50m)),
                     a, am, int(n80), int(round(n80m)), b, bm])

order = {"state": 0, "county": 1, "town": 2, "tract": 3}
rows.sort(key=lambda r: (order[r[1]], r[0]))
with open(OUT, "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f, lineterminator="\n")
    w.writerow(["geoid", "level", "name", "housing_units", "housing_units_moe",
                "built_pre1950", "built_pre1950_moe", "pre1950_pct", "pre1950_pct_moe",
                "built_pre1980", "built_pre1980_moe", "pre1980_pct", "pre1980_pct_moe"])
    w.writerows(rows)
print(len(rows), "rows; missing:", len(set(wanted) - {r[0] for r in rows}))
