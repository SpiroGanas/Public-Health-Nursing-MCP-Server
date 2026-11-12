#!/usr/bin/env python3
"""Check the live formulas of the budget workbook (build_program_budget, PLAN-09).

Usage:
  BUDGET_DIR=/tmp/budget_wb cargo test -p phn-tools --test budget_golden -- --ignored
  python3 -I check_budget_workbook.py /tmp/budget_wb

Needs LibreOffice (soffice) and openpyxl. Each workbook is recalculated by LibreOffice (the load
setting 'always recalculate' is set in a throwaway profile, so the values come from the formulas,
not from the cached results the Rust code wrote) and the Budget summary cells are compared with
budget_golden.json, which budget_golden.py computes independently. Not part of CI.
"""
import json
import subprocess
import sys
import tempfile
from pathlib import Path

import openpyxl

HERE = Path(__file__).resolve().parent
FIXTURE = json.loads((HERE.parent / "budget_golden.json").read_text())

# Row offsets inside a summary block (the block header is at `top`, classes start at top + 1).
ROWS = ["personnel", "fringe", "travel", "equipment", "supplies", "contractual", "other",
        "direct", "indirect_base", "indirect", "total"]
TOPS = {"federal": 4, "cost_share": 17}


def recalc(src: Path, outdir: Path) -> Path:
    profile = Path(tempfile.mkdtemp(prefix="lo_profile_"))
    user = profile / "user"
    user.mkdir(parents=True)
    (user / "registrymodifications.xcu").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<oor:items xmlns:oor="http://openoffice.org/2001/registry" xmlns:xs="http://www.w3.org/2001/XMLSchema" '
        'xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">\n'
        '<item oor:path="/org.openoffice.Office.Calc/Formula/Load"><prop oor:name="OOXMLRecalcMode" oor:op="fuse"><value>0</value></prop></item>\n'
        "</oor:items>\n"
    )
    outdir.mkdir(exist_ok=True)
    subprocess.run(
        ["soffice", f"-env:UserInstallation=file://{profile}", "--headless", "--convert-to", "xlsx",
         "--outdir", str(outdir), str(src)],
        check=True, capture_output=True, timeout=240,
    )
    return outdir / src.name


def main(folder: str) -> int:
    folder = Path(folder)
    out = folder / "recalculated"
    bad = 0
    checked = 0
    for case in FIXTURE["cases"]:
        name = case["name"]
        src = folder / f"{name}.xlsx"
        if not src.exists():
            print(f"missing {src}")
            return 2
        wb = openpyxl.load_workbook(recalc(src, out), data_only=True)
        ws = wb["Budget summary"]
        n = case["years"]
        for funding, top in TOPS.items():
            exp = case["expected"][funding]
            for i, key in enumerate(ROWS):
                row = top + 1 + i
                for k in range(n):
                    got = ws.cell(row=row, column=2 + k).value
                    want = float(exp[key][k])
                    checked += 1
                    if got is None or abs(float(got) - want) > 1e-6:
                        bad += 1
                        print(f"{name} {funding} {key} year {k + 1}: workbook {got} vs expected {want}")
                total = ws.cell(row=row, column=2 + n).value
                want_total = sum(float(x) for x in exp[key])
                checked += 1
                if total is None or abs(float(total) - want_total) > 1e-6:
                    bad += 1
                    print(f"{name} {funding} {key} total: workbook {total} vs expected {want_total}")
        m = case["expected"].get("match")
        if m:
            for cell, key in (("B44", "required"), ("B45", "budgeted"), ("B46", "shortfall")):
                got = ws[cell].value
                checked += 1
                if got is None or abs(float(got) - float(m[key])) > 0.006:
                    bad += 1
                    print(f"{name} match {key}: workbook {got} vs expected {m[key]}")
        status = ws["B50"].value
        checked += 1
        if status != "OK":
            bad += 1
            print(f"{name}: arithmetic check cell says {status!r}")
    print(f"{checked} cells checked, {bad} differences")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
