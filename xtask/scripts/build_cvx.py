"""Convert CDC IIS code set flat files (pipe-delimited) to clean CSV snapshots.

usage: build_cvx.py SRC_DIR OUT_DIR
SRC_DIR holds cvx.txt, mvx.txt, tradename.txt, ndc_sale.txt, ndc_use.txt as downloaded from
https://www2a.cdc.gov/vaccines/iis/iisstandards/downloads/ .
"""
import csv, os, sys

SRC, OUT = sys.argv[1:3]


def rows(name, n):
    with open(os.path.join(SRC, name), encoding="utf-8-sig", errors="replace") as f:
        for line in f.read().splitlines():
            if not line.strip():
                continue
            parts = [p.strip() for p in line.split("|")]
            if len(parts) < n:
                parts += [""] * (n - len(parts))
            yield parts


def write(name, header, data):
    data.sort()
    with open(os.path.join(OUT, name), "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f, lineterminator="\n")
        w.writerow(header)
        w.writerows(data)
    print(name, len(data))


def date(s):
    s = s.strip()
    if len(s) == 8 and s.isdigit():
        return f"{s[:4]}-{s[4:6]}-{s[6:]}"
    return s.replace("/", "-")[:10]


cvx = [[p[0], p[1], p[2], p[3], p[4], p[5], date(p[6])] for p in rows("cvx.txt", 7)]
write("cdc_cvx.csv", ["cvx", "short_description", "full_name", "notes", "status", "non_vaccine", "last_updated"],
      [[c[0].zfill(2) if c[0].isdigit() else c[0]] + c[1:] for c in cvx])
mvx = [[p[0], p[1], p[2], p[3], date(p[4])] for p in rows("mvx.txt", 5)]
write("cdc_mvx.csv", ["mvx", "manufacturer", "notes", "status", "last_updated"], mvx)
tn = [[p[0], p[1], p[2].zfill(2) if p[2].isdigit() else p[2], p[3], p[4], p[5], p[6], date(p[7])]
      for p in rows("tradename.txt", 9)]
write("cdc_vaccine_tradenames.csv",
      ["trade_name", "cvx_description", "cvx", "manufacturer", "mvx", "mvx_status", "product_status", "last_updated"], tn)
ndc = []
for p in rows("ndc_sale.txt", 16):
    ndc.append([p[14], "sale", p[4], p[6], date(p[7]), date(p[8]), p[9], p[10], p[12].zfill(2) if p[12].isdigit() else p[12]])
for p in rows("ndc_use.txt", 17):
    ndc.append([p[14], "use", p[4], p[6], date(p[7]), date(p[8]), p[9], "", p[11].zfill(2) if p[11].isdigit() else p[11]])
write("cdc_vaccine_ndc.csv",
      ["ndc", "level", "proprietary_name", "labeler", "start_date", "end_date", "packaging", "route", "cvx"], ndc)
