#!/usr/bin/env python3
"""Check the live formulas of the QI tracker workbook (plan_quality_improvement, EBP-05).

Usage:
  QI_TRACKER_DIR=/tmp/qi_trackers cargo test -p phn-tools --lib golden_workbooks -- --ignored
  python3 -I check_qi_tracker.py /tmp/qi_trackers

Needs LibreOffice (soffice) and openpyxl. Each workbook is recalculated by LibreOffice (the load
setting 'always recalculate' is set in a throwaway profile) and the values of the Data and Summary
sheets are compared with an independent Python computation of the same rules. Not part of CI.
"""
import itertools
import json
import math
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import openpyxl

HERE = Path(__file__).resolve().parent
FIXTURE = json.loads((HERE.parent / "qi_run_chart_golden.json").read_text())


def recalc(src: Path, outdir: Path):
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
        ["soffice", f"-env:UserInstallation=file://{profile}", "--headless", "--convert-to", "xlsx", "--outdir", str(outdir), str(src)],
        check=True, capture_output=True, timeout=180,
    )
    return outdir / src.name


def median(xs):
    s = sorted(xs)
    n = len(s)
    return s[n // 2] if n % 2 else (s[n // 2 - 1] + s[n // 2]) / 2


def expected_series(values, baseline):
    med = median(values[:baseline])
    side = [0 if v == med else (1 if v > med else -1) for v in values]
    # running run length (points on the median keep the previous length)
    run_len, last, cur = [], 0, 0
    new_run = []
    for s in side:
        if s == 0:
            run_len.append(cur)
            new_run.append(0)
        elif s == last:
            cur += 1
            run_len.append(cur)
            new_run.append(0)
        else:
            cur = 1
            last = s
            run_len.append(cur)
            new_run.append(1)
    # running trend length with ties collapsed
    tl, td = [1], [0]
    for i in range(1, len(values)):
        if values[i] == values[i - 1]:
            tl.append(tl[-1]); td.append(td[-1])
        else:
            d = 1 if values[i] > values[i - 1] else -1
            tl.append(tl[-1] + 1 if d == td[-1] else 2)
            td.append(d)
    return med, side, run_len, new_run, tl


def check_rule_workbooks(dirpath: Path):
    out = Path(tempfile.mkdtemp(prefix="qi_recalc_"))
    bad = 0
    for case in FIXTURE["rule_cases"]:
        name = case["name"]
        values = case["values"]
        n = len(values)
        base = case["baseline"] or n
        conv = recalc(dirpath / f"{name}.xlsx", out)
        wb = openpyxl.load_workbook(conv, data_only=True)
        ws, sm = wb["Data"], wb["Summary"]
        med, side, run_len, new_run, tl = expected_series(values, base)
        exp = case["expected"]
        errs = []
        got = lambda col: [ws[f"{col}{r}"].value for r in range(2, n + 2)]
        if any(abs(float(v) - med) > 1e-9 for v in got("E")):
            errs.append(("median", got("E")[:2], med))
        if [int(v) for v in got("F")] != side:
            errs.append(("side", got("F"), side))
        if [int(v) for v in got("H")] != run_len:
            errs.append(("run length", got("H"), run_len))
        if [int(v) for v in got("J")] != tl:
            errs.append(("trend length", got("J"), tl))
        shift_flags = [i for i, v in enumerate(got("I")) if v == "Shift"]
        want_shift = sorted(i for _, g in itertools.groupby([(i, s) for i, s in enumerate(side) if s], key=lambda t: t[1])
                            for i in [k for k, _ in list(g)][5:])
        if shift_flags != want_shift:
            errs.append(("shift flags", shift_flags, want_shift))
        # Summary
        cells = {r: sm[f"B{r}"].value for r in range(3, 25)}
        if int(cells[3]) != n:
            errs.append(("periods", cells[3], n))
        if int(cells[4]) != exp["above"] or int(cells[5]) != exp["below"]:
            errs.append(("above/below", cells[4], cells[5]))
        if int(cells[8]) != exp["longest_run"]:
            errs.append(("longest run", cells[8], exp["longest_run"]))
        if int(cells[9]) != exp["runs"]:
            errs.append(("runs", cells[9], exp["runs"]))
        if int(cells[11]) != exp["longest_trend"]:
            errs.append(("longest trend", cells[11], exp["longest_trend"]))
        useful = exp["above"] + exp["below"]
        lim_run = round(math.log2(useful) + 3) if useful >= 2 else None
        # qbinom(0.05, n-1, 0.5)
        lim_cross = None
        if useful >= 2:
            cum = 0.0
            for k in range(useful):
                cum += math.comb(useful - 1, k) / 2 ** (useful - 1)
                if cum >= 0.05:
                    lim_cross = k
                    break
        if lim_run is not None and (int(cells[19]) != lim_run or int(cells[20]) != lim_cross):
            errs.append(("anhoej limits", cells[19], cells[20], lim_run, lim_cross))
        if (str(cells[13]).startswith("Signal")) != (exp["longest_run"] >= 6):
            errs.append(("shift signal", cells[13]))
        if (str(cells[14]).startswith("Signal")) != (exp["longest_trend"] >= 5):
            errs.append(("trend signal", cells[14]))
        # Runs test uses the table (even split); compare with the case's own limits only when the split is even.
        if exp["above"] == exp["below"] or abs(exp["above"] - exp["below"]) <= 1:
            few = exp["signal_if_runs_at_most"]
            many = exp["signal_if_runs_at_least"]
            want = "Signal: too few runs" if (few is not None and exp["runs"] <= few) else (
                "Signal: too many runs" if (many is not None and exp["runs"] >= many) else "No runs signal")
            if cells[17] != want and 8 <= useful <= 100:
                errs.append(("runs test", cells[17], want))
        print(("FAIL " if errs else "ok   ") + name, errs if errs else "")
        bad += bool(errs)
    return bad


def check_control(dirpath: Path):
    out = Path(tempfile.mkdtemp(prefix="qi_recalc_"))
    bad = 0
    # p chart
    p = FIXTURE["control"]["p"][0]
    conv = recalc(dirpath / "control_p0.xlsx", out)
    ws = openpyxl.load_workbook(conv, data_only=True)["Data"]
    n = len(p["numerators"])
    for i in range(n):
        r = i + 2
        center, lcl, ucl = ws[f"O{r}"].value, ws[f"P{r}"].value, ws[f"Q{r}"].value
        e = p["expected"]
        ok = abs(center - e["center"]) < 1e-9 and abs(lcl - e["lcl"][i]) < 1e-9 and abs(ucl - e["ucl"][i]) < 1e-9
        if not ok:
            print("FAIL p chart row", r, center, lcl, ucl, e["center"], e["lcl"][i], e["ucl"][i])
            bad += 1
    print("ok   p chart limits" if not bad else "FAIL p chart")
    # xmr
    x = FIXTURE["control"]["xmr"][0]
    conv = recalc(dirpath / "control_xmr0.xlsx", out)
    ws = openpyxl.load_workbook(conv, data_only=True)["Data"]
    e = x["expected"]
    b2 = 0
    for i in range(len(x["values"])):
        r = i + 2
        center, lcl, ucl = ws[f"O{r}"].value, ws[f"P{r}"].value, ws[f"Q{r}"].value
        if not (abs(center - e["center"]) < 1e-9 and abs(lcl - e["lcl"]) < 1e-9 and abs(ucl - e["ucl"]) < 1e-9):
            print("FAIL xmr row", r, center, lcl, ucl, e)
            b2 += 1
    print("ok   xmr limits" if not b2 else "FAIL xmr")
    return bad + b2


if __name__ == "__main__":
    d = Path(sys.argv[1])
    failures = check_rule_workbooks(d) + check_control(d)
    sys.exit(1 if failures else 0)
