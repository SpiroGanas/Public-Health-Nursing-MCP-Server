#!/usr/bin/env python3
"""Independent reference values for track_program_performance (PLAN-11).

Usage (repository root, Python 3.9+, standard library only):

    python3 -I crates/phn-server/tests/fixtures/phase16_interventions/generate_expected.py [--check]

Reads fixtures/synthetic/data/program_metrics.csv and computes, without any of the server's code,
what the tool must report for the targets used in phase16_interventions_tools.rs: cumulative
totals, pooled percentages, progress to target, pace (expected by now and status), which periods
the small-cell rule hides at a threshold of 5, and whether the run-chart rules (Anhoej J, Olesen
AV, PLoS One 2014;9:e113825: longest run above round(log2(n) + 3) useful observations, or fewer
crossings of the median than the 5th percentile of Binomial(n - 1, 0.5)) signal. Writes
expected_performance.json beside this script. With --check it writes nothing and exits 1 if the
file on disk differs.
"""
import csv
import json
import math
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
SRC = ROOT / "fixtures/synthetic/data/program_metrics.csv"
THRESHOLD = 5
PERIODS_IN_PROGRAM = 12
ON_TRACK = 0.9


def rows():
    with open(SRC, newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def median(v):
    s = sorted(v)
    n = len(s)
    return s[n // 2] if n % 2 else (s[n // 2 - 1] + s[n // 2]) / 2


def qbinom(p, n, q=0.5):
    cdf = 0.0
    for k in range(n + 1):
        cdf += math.comb(n, k) * q**k * (1 - q) ** (n - k)
        if cdf >= p:
            return k
    return n


def anhoej(values):
    med = median(values)
    sides = [1 if x > med else -1 if x < med else 0 for x in values]
    useful = [s for s in sides if s != 0]
    n = len(useful)
    longest, run, prev = 0, 0, 0
    crossings = 0
    for s in useful:
        if s == prev:
            run += 1
        else:
            if prev != 0:
                crossings += 1
            run = 1
        prev = s
        longest = max(longest, run)
    limit_run = math.floor(math.log2(max(n, 1)) + 3 + 0.5)
    limit_cross = qbinom(0.05, n - 1) if n >= 2 else 0
    return {
        "useful": n,
        "longest_run": longest,
        "longest_run_limit": limit_run,
        "crossings": crossings,
        "crossings_limit": limit_cross,
        "signal": longest > limit_run or crossings < limit_cross,
    }


def small(x):
    return 0 < x < THRESHOLD


def build():
    r = rows()
    out = {"threshold": THRESHOLD, "periods_in_program": PERIODS_IN_PROGRAM, "measures": {}}
    # Clients enrolled: count, target 400, cumulative.
    v = [float(x["Clients enrolled"]) for x in r]
    total, target = sum(v), 400.0
    expected = target * len(v) / PERIODS_IN_PROGRAM
    status = "Met" if total >= target else ("On track" if total >= ON_TRACK * expected else "Behind")
    out["measures"]["Clients enrolled"] = {
        "periods": len(v), "current": total, "progress_percent": round(total / target * 100, 2),
        "expected_by_now": round(expected, 2), "status": status,
        "hidden_periods": [i for i, x in enumerate(v) if small(x)],
        "run_chart": anhoej(v),
    }
    # Screening rate: pooled percent, target 80.
    num = [float(x["Participants screened"]) for x in r]
    den = [float(x["Participants eligible"]) for x in r]
    pooled = sum(num) / sum(den) * 100
    hidden = [i for i in range(len(num)) if small(num[i]) or small(den[i]) or small(den[i] - num[i])]
    out["measures"]["Participants screened"] = {
        "periods": len(num), "current": round(pooled, 2),
        "progress_percent": round(pooled / 80 * 100, 2),
        "status": "Met" if pooled >= 80 else "Below target",
        "mean_of_period_percentages": round(sum(a / b * 100 for a, b in zip(num, den)) / len(num), 2),
        "hidden_periods": hidden,
        "run_chart": anhoej([a / b * 100 for a, b in zip(num, den)]),
    }
    # Sessions held: a steady rise (the run-chart rules signal), target 130.
    c = [float(x["Sessions held"]) for x in r]
    out["measures"]["Sessions held"] = {
        "periods": len(c), "current": sum(c), "hidden_periods": [i for i, x in enumerate(c) if small(x)],
        "withheld": any(small(x) for x in c), "run_chart": anhoej(c),
        "status": "Met" if sum(c) >= 130 else ("On track" if sum(c) >= ON_TRACK * 130 * len(c) / PERIODS_IN_PROGRAM else "Behind"),
    }
    # Housing referrals: small counts hidden.
    h = [float(x["Housing referrals"]) for x in r]
    out["measures"]["Housing referrals"] = {
        "periods": len(h), "current": sum(h), "hidden_periods": [i for i, x in enumerate(h) if small(x)],
        "withheld": any(small(x) for x in h),
    }
    return out


def main():
    text = json.dumps(build(), indent=1) + "\n"
    path = HERE / "expected_performance.json"
    if "--check" in sys.argv:
        same = path.exists() and path.read_text() == text
        print("unchanged" if same else "changed")
        sys.exit(0 if same else 1)
    path.write_text(text)
    print(text)


if __name__ == "__main__":
    main()
