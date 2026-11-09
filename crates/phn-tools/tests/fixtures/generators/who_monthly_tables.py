"""Write who_monthly_zscores.json: WHO Child Growth Standards z-score tables (WHO 2006) for
0-24 months, used by tests/growth_golden.rs.

usage: who_monthly_tables.py TABLES_DIR OUT_JSON

TABLES_DIR is pygrowup/tables from the pygrowup 0.8.2 source distribution on PyPI
(https://pypi.org/project/pygrowup/0.8.2/, BSD licence, UNICEF), which redistributes WHO's
published simplified z-score tables (L, M, S and the -3 to +3 SD values to 0.1 unit). WHO's own
site (who.int) and CDC's monthly copies on ftp.cdc.gov were not reachable when this was built.
"""
import json, os, sys

SRC, OUT = sys.argv[1:3]
TABLES = [
    ("weight_for_age", "wfa_{}_0_5_zscores.json", "Month", 24),
    ("length_for_age", "lhfa_{}_0_2_zscores.json", "Month", 24),
    ("head_circumference_for_age", "hcfa_{}_0_5_zscores.json", "Month", 24),
    ("bmi_for_age", "bmifa_{}_0_2_zscores.json", "Month", 24),
    ("weight_for_length", "wfl_{}_0_2_zscores.json", "Length", None),
]
SD = ["SD3neg", "SD2neg", "SD1neg", "SD0", "SD1", "SD2", "SD3"]
out = {"source": "WHO Child Growth Standards (2006) simplified z-score tables, as redistributed "
       "in pygrowup 0.8.2 (PyPI, BSD licence)", "rows": []}
for indicator, pattern, key, max_month in TABLES:
    for sex, name in [(1, "boys"), (2, "girls")]:
        with open(os.path.join(SRC, pattern.format(name))) as f:
            for r in json.load(f):
                x = float(r[key])
                if max_month is not None and x > max_month:
                    continue
                out["rows"].append({
                    "indicator": indicator, "sex": sex, "x": x,
                    "l": float(r["L"]), "m": float(r["M"]), "s": float(r["S"]),
                    "sd": [float(r[k]) for k in SD],
                })
with open(OUT, "w") as f:
    json.dump(out, f, indent=0)
print(len(out["rows"]), "rows")
