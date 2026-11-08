"""Build the growth chart snapshots for calculate_growth_percentiles (MCH-03).

usage: build_growth_charts.py SRC_DIR OUT_DIR

SRC_DIR holds the files as downloaded from cdc.gov (plain curl works):

  https://www.cdc.gov/growthcharts/data/zscore/wtage.csv
  https://www.cdc.gov/growthcharts/data/zscore/statage.csv
  https://www.cdc.gov/growthcharts/data/zscore/hcageinf.csv
  https://www.cdc.gov/growthcharts/data/extended-bmi/bmi-age-2022.csv
  https://www.cdc.gov/growth-chart-training/media/files/WHOref_d.csv

The four CDC 2000 / extended BMI files are copied verbatim (they carry the LMS parameters and
CDC's published percentiles, which the golden tests reproduce). WHOref_d.csv is CDC's copy of
the WHO Child Growth Standards by day of age (used by CDC's SAS program for the WHO charts);
CDC's monthly WHO files are on ftp.cdc.gov, which was not reachable. From it we keep:

  who_growth_by_age.csv   sex, age in days 0-730, and the L, M, S of weight, length,
                          head circumference, and BMI ("forage" rows)
  who_growth_wfl.csv      sex, length 45.0-110.0 cm every 0.1 cm, and the L, M, S of
                          weight-for-length ("forlen" rows; the source's 0.01 cm grid is linear
                          between 0.1 cm points to within 1e-8, so the tool interpolates)

Values are copied as text, unchanged. The script prints each output's SHA-256 and row count
for snapshots/manifest.json.
"""
import csv, hashlib, os, shutil, sys

SRC, OUT = sys.argv[1:3]

VERBATIM = {
    "wtage.csv": "cdc_growth_wtage.csv",
    "statage.csv": "cdc_growth_statage.csv",
    "hcageinf.csv": "cdc_growth_hcageinf.csv",
    "bmi-age-2022.csv": "cdc_growth_bmi_age_2022.csv",
}


def digest(path):
    with open(path, "rb") as f:
        b = f.read()
    rows = b.count(b"\n") - 1 if b.endswith(b"\n") else b.count(b"\n")
    return hashlib.sha256(b).hexdigest(), len(b), rows


for src, dst in VERBATIM.items():
    shutil.copyfile(os.path.join(SRC, src), os.path.join(OUT, dst))

with open(os.path.join(SRC, "WHOref_d.csv"), newline="", encoding="utf-8-sig") as f:
    ref = list(csv.DictReader(f))

by_age = [
    r for r in ref
    if r["_denom"] == "forage" and r["_agedays"] != "" and int(r["_agedays"]) <= 730
]
by_age.sort(key=lambda r: (int(r["sex"]), int(r["_agedays"])))
with open(os.path.join(OUT, "who_growth_by_age.csv"), "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f, lineterminator="\n")
    w.writerow(["sex", "agedays", "wt_l", "wt_m", "wt_s", "len_l", "len_m", "len_s",
                "hc_l", "hc_m", "hc_s", "bmi_l", "bmi_m", "bmi_s"])
    for r in by_age:
        w.writerow([r["sex"], r["_agedays"],
                    r["_wei_l"], r["_wei_m"], r["_wei_s"],
                    r["_len_l"], r["_len_m"], r["_len_s"],
                    r["_headc_l"], r["_headc_m"], r["_headc_s"],
                    r["_bmi_l"], r["_bmi_m"], r["_bmi_s"]])

wfl = []
for r in ref:
    if r["_denom"] != "forlen":
        continue
    tenths = round(float(r["_len"]) * 100)
    if tenths % 10 == 0:
        wfl.append((int(r["sex"]), tenths, r))
wfl.sort(key=lambda t: (t[0], t[1]))
with open(os.path.join(OUT, "who_growth_wfl.csv"), "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f, lineterminator="\n")
    w.writerow(["sex", "length_cm", "l", "m", "s"])
    for sex, tenths, r in wfl:
        w.writerow([sex, f"{tenths / 100:.1f}", r["_wfl_l"], r["_wfl_m"], r["_wfl_s"]])

print("source WHOref_d.csv", digest(os.path.join(SRC, "WHOref_d.csv")))
for name in list(VERBATIM.values()) + ["who_growth_by_age.csv", "who_growth_wfl.csv"]:
    print(name, digest(os.path.join(OUT, name)))
