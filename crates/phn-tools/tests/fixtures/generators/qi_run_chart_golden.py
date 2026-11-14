#!/usr/bin/env python3
"""Golden values for crates/phn-tools/src/ebp/qi/rules.rs (EBP-05 plan_quality_improvement).

Run:  python3 -I qi_run_chart_golden.py > ../qi_run_chart_golden.json
      python3 -I qi_run_chart_golden.py --extract-anhoj-table anhoj2014_pmc.xml > ../qi_anhoj_table1.json
      (the second form reads the PubMed Central XML of Table 1 of the 2014 PLoS One paper,
       fetched with NCBI E-utilities efetch db=pmc id=PMC4244133, and keeps its numbers.)

Every value is computed here from the DEFINITIONS, with code that is deliberately
different from the Rust implementation (itertools.groupby, exact integer counts, brute-force
enumeration of arrangements), so the Rust tests compare two independent computations.

Definitions (the sources are cited in content/ebp/quality_improvement.yaml):
* median of the baseline points; points on the median are skipped: they neither add to a run
  nor break it (PLoS One 2014;9:e113825, guideline step 5);
* shift: 6 or more consecutive points on one side of the median (Perla, as described in
  PLoS One 2014;9:e113825 and PLoS One 2015;10:e0121349);
* trend: 5 or more consecutive points going up or all going down; a repeated value counts
  once (PLoS One 2015;10:e0121349, results: "longest trend > 4 ... repeating values count as
  one");
* runs: the number of runs of points on one side of the median; the exact distribution of the
  number of runs given n1 points above and n2 below (Wald-Wolfowitz); a signal for too few
  (at most a) or too many (at least b) runs has probability at most 0.025 in each tail;
* astronomical candidate: a point whose modified z-score against the OTHER points is above 3.5
  (this tool's screen; Perla's rule is a judgment call, see astronomical() below).
"""
import itertools
import json
import math
import random
import sys
from fractions import Fraction

TAIL = Fraction(1, 40)  # 2.5% in each tail


def median(xs):
    s = sorted(xs)
    n = len(s)
    return float(s[n // 2]) if n % 2 else (s[n // 2 - 1] + s[n // 2]) / 2.0


def sides(values, med):
    out = []
    for v in values:
        out.append(0 if v == med else (1 if v > med else -1))
    return out


def runs_of_sides(sd):
    """List of (side, [indices]) for off-median points, points on the median skipped."""
    seq = [(i, s) for i, s in enumerate(sd) if s != 0]
    runs = []
    for side, grp in itertools.groupby(seq, key=lambda t: t[1]):
        runs.append((side, [i for i, _ in grp]))
    return runs


def trends(values):
    """Maximal monotone stretches (>= 2 distinct values) with ties collapsed.

    Returns a list of dicts with the original indices covered and the number of distinct
    points (ties count once).
    """
    # collapse consecutive equal values, remembering the original indices
    groups = []
    for v, grp in itertools.groupby(enumerate(values), key=lambda t: t[1]):
        idx = [i for i, _ in grp]
        groups.append((v, idx))
    out = []
    if len(groups) < 2:
        return out
    diffs = [1 if groups[k + 1][0] > groups[k][0] else -1 for k in range(len(groups) - 1)]
    k = 0
    while k < len(diffs):
        j = k
        while j + 1 < len(diffs) and diffs[j + 1] == diffs[k]:
            j += 1
        members = groups[k : j + 2]
        idx = [i for _, ix in members for i in ix]
        out.append({"direction": diffs[k], "distinct": len(members), "indices": idx})
        k = j + 1
    return out


def runs_distribution_counts(n1, n2):
    """Exact counts of arrangements by number of runs, by brute force for N <= 20 else by the
    closed form with exact integers (math.comb)."""
    N = n1 + n2
    total = math.comb(N, n1)
    counts = {}
    if N <= 20:
        for pos in itertools.combinations(range(N), n1):
            s = [0] * N
            for p in pos:
                s[p] = 1
            r = 1 + sum(1 for a, b in zip(s, s[1:]) if a != b)
            counts[r] = counts.get(r, 0) + 1
    else:
        for r in range(2, N + 1):
            k = r // 2
            if r % 2 == 0:
                c = 2 * math.comb(n1 - 1, k - 1) * math.comb(n2 - 1, k - 1)
            else:
                c = math.comb(n1 - 1, k) * math.comb(n2 - 1, k - 1) + math.comb(n1 - 1, k - 1) * math.comb(n2 - 1, k)
            if c:
                counts[r] = c
    assert sum(counts.values()) == total, (n1, n2, sum(counts.values()), total)
    return counts, total


def runs_limits(n1, n2):
    if n1 == 0 or n2 == 0:
        return None, None
    counts, total = runs_distribution_counts(n1, n2)
    rs = sorted(counts)
    a = None
    cum = 0
    for r in rs:
        cum += counts[r]
        if Fraction(cum, total) <= TAIL:
            a = r
        else:
            break
    b = None
    cum = 0
    for r in reversed(rs):
        cum += counts[r]
        if Fraction(cum, total) <= TAIL:
            b = r
        else:
            break
    return a, b


def astronomical(values):
    """Screen for an astronomical-point candidate: the modified z-score of the point against
    the OTHER points, 0.6745 * (x - median(rest)) / MAD(rest), is above 3.5 in size (a common
    outlier rule). Skipped when the others have no spread (MAD = 0) or there are fewer than
    10 points."""
    n = len(values)
    if n < 10:
        return []
    out = []
    for i in range(n):
        rest = values[:i] + values[i + 1 :]
        m = median(rest)
        mad = median([abs(x - m) for x in rest])
        if mad <= 0:
            continue
        if abs(0.6745 * (values[i] - m) / mad) > 3.5:
            out.append(i)
    return out


def analyze(values, baseline=None, shift_len=6, trend_len=5):
    base = values[: (baseline or len(values))]
    med = median(base)
    sd = sides(values, med)
    rs = runs_of_sides(sd)
    n1 = sum(1 for s in sd if s > 0)
    n2 = sum(1 for s in sd if s < 0)
    shift_pts = sorted(i for _, ix in rs if len(ix) >= shift_len for i in ix)
    longest_run = max((len(ix) for _, ix in rs), default=0)
    tr = trends(values)
    trend_pts = sorted({i for t in tr if t["distinct"] >= trend_len for i in t["indices"]})
    longest_trend = max((t["distinct"] for t in tr), default=0)
    a, b = runs_limits(n1, n2)
    nruns = len(rs)
    return {
        "median": med,
        "above": n1,
        "below": n2,
        "on_median": sum(1 for s in sd if s == 0),
        "longest_run": longest_run,
        "shift_points": shift_pts,
        "longest_trend": longest_trend,
        "trend_points": trend_pts,
        "runs": nruns,
        "signal_if_runs_at_most": a,
        "signal_if_runs_at_least": b,
        "too_few_runs": a is not None and nruns <= a,
        "too_many_runs": b is not None and nruns >= b,
        "astronomical": astronomical(values),
    }


def xmr(values, baseline=None):
    base = values[: (baseline or len(values))]
    mean = sum(base) / len(base)
    mr = [abs(base[k + 1] - base[k]) for k in range(len(base) - 1)]
    mrbar = sum(mr) / len(mr)
    sigma = mrbar / 1.128  # d2 for moving ranges of 2 (3 / 1.128 = 2.66 rounded)
    return {"center": mean, "lcl": mean - 3 * sigma, "ucl": mean + 3 * sigma, "mrbar": mrbar}


def pchart(nums, dens):
    pbar = sum(nums) / sum(dens)
    return {
        "center": pbar,
        "lcl": [max(0.0, pbar - 3 * math.sqrt(pbar * (1 - pbar) / d)) for d in dens],
        "ucl": [min(1.0, pbar + 3 * math.sqrt(pbar * (1 - pbar) / d)) for d in dens],
    }


def uchart(counts, expo):
    ubar = sum(counts) / sum(expo)
    return {
        "center": ubar,
        "lcl": [max(0.0, ubar - 3 * math.sqrt(ubar / e)) for e in expo],
        "ucl": [ubar + 3 * math.sqrt(ubar / e) for e in expo],
    }


def cchart(counts):
    cbar = sum(counts) / len(counts)
    return {"center": cbar, "lcl": max(0.0, cbar - 3 * math.sqrt(cbar)), "ucl": cbar + 3 * math.sqrt(cbar)}


def main():
    rng = random.Random(20261112)
    series = {
        # 10 below then 10 above the median: a shift of 10 and 2 runs.
        "step_change": [4, 5, 4, 6, 5, 4, 5, 6, 4, 5, 9, 10, 9, 11, 10, 9, 10, 11, 9, 10],
        # strictly rising: a trend of 12 points, a single shift on each side.
        "steady_climb": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        # ties inside a trend: 1,2,2,3,4,5 has 5 distinct points rising.
        "trend_with_tie": [3, 1, 2, 2, 3, 4, 5, 4, 6, 3, 5, 4],
        # points on the median are skipped: median 5 occurs several times.
        "on_median": [5, 6, 7, 5, 6, 7, 6, 7, 5, 4, 3, 5, 4, 3, 4, 3, 5, 6],
        # alternating: too many runs.
        "alternating": [3, 8, 2, 9, 3, 8, 2, 9, 3, 8, 2, 9, 3, 8, 2, 9],
        # an outlier among stable points: astronomical candidate at index 7.
        "outlier": [10, 11, 9, 10, 12, 10, 11, 31, 10, 9, 11, 10, 12, 9],
        # shift only after the baseline (median from the first 8 points).
        "post_baseline_shift": [7, 8, 6, 7, 8, 6, 7, 8, 11, 12, 11, 13, 12, 11, 12, 13, 11, 12],
        "short": [5, 6, 4, 7, 5, 6, 4, 5],
    }
    for k in range(6):
        series[f"random_{k}"] = [rng.randint(20, 60) for _ in range(rng.randint(14, 40))]
    cases = []
    for name, v in series.items():
        base = 8 if name == "post_baseline_shift" else None
        cases.append({"name": name, "values": v, "baseline": base, "expected": analyze(v, base)})
    limit_cases = []
    for n1, n2 in [(5, 5), (6, 6), (8, 7), (10, 10), (12, 8), (9, 9), (15, 15), (20, 20), (31, 29), (50, 50), (100, 100)]:
        a, b = runs_limits(n1, n2)
        limit_cases.append({"n_above": n1, "n_below": n2, "signal_if_at_most": a, "signal_if_at_least": b})
    control = {
        "xmr": [],
        "p": [],
        "u": [],
        "c": [],
    }
    for k in range(3):
        v = [round(rng.gauss(50, 6), 2) for _ in range(rng.randint(12, 24))]
        control["xmr"].append({"values": v, "baseline": None, **{"expected": xmr(v)}})
    v = [round(rng.gauss(50, 6), 2) for _ in range(20)]
    control["xmr"].append({"values": v, "baseline": 12, "expected": xmr(v, 12)})
    for k in range(2):
        dens = [rng.randint(40, 120) for _ in range(15)]
        nums = [int(d * rng.uniform(0.3, 0.5)) for d in dens]
        control["p"].append({"numerators": nums, "denominators": dens, "expected": pchart(nums, dens)})
        expo = [round(rng.uniform(300, 900), 1) for _ in range(15)]
        cnt = [rng.randint(0, 9) for _ in expo]
        control["u"].append({"counts": cnt, "exposure": expo, "expected": uchart(cnt, expo)})
        cc = [rng.randint(6, 26) for _ in range(16)]
        control["c"].append({"counts": cc, "expected": cchart(cc)})
    out = {
        "description": "Generated by tests/fixtures/generators/qi_run_chart_golden.py; see its docstring for the definitions.",
        "rule_cases": cases,
        "runs_limit_cases": limit_cases,
        "control": control,
    }
    json.dump(out, sys.stdout, indent=1)
    print()


def extract_anhoj_table(path):
    import xml.etree.ElementTree as ET

    root = ET.parse(path).getroot()
    rows = []
    for t in root.iter("table-wrap"):
        lab = "".join(t.find("label").itertext())
        if lab.strip() != "Table 1":
            continue
        for tr in t.iter("tr"):
            cells = [" ".join("".join(td.itertext()).split()) for td in tr]
            if cells and cells[0].isdigit():
                rows.append({"n": int(cells[0]), "longest_run_limit": int(cells[1]), "crossings_limit": int(cells[2])})
    json.dump(
        {
            "description": "Table 1 of PLoS One 2014;9(11):e113825 (doi:10.1371/journal.pone.0113825, CC BY 4.0): "
            "prediction limits for the longest run, round(log2(n)+3), and the number of crossings, "
            "qbinom(0.05, n-1, 0.5), by the number of useful observations n.",
            "rows": rows,
        },
        sys.stdout,
        indent=1,
    )
    print()


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--extract-anhoj-table":
        extract_anhoj_table(sys.argv[2])
    else:
        main()
