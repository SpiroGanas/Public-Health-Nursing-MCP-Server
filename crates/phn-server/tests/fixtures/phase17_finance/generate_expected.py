#!/usr/bin/env python3
"""Independent reference values for analyze_health_department_budget (LEAD-01) and
model_staffing_needs (LEAD-02).

Usage (repository root, Python 3.9+, standard library only):

    python3 -I crates/phn-server/tests/fixtures/phase17_finance/generate_expected.py [--check]

Reads the synthetic fixtures in fixtures/synthetic/data/ (health_dept_budget.csv,
health_dept_budget_small.csv, health_dept_ledger_monthly.csv) and computes, without any of the
server's code, what the tools must report for the arguments used in phase17_finance_tools.rs:

* the fiscal year (July 1 start, named for the year it ends), days elapsed counting the as-of
  date, and the share of the year elapsed;
* for each expense and revenue category: budget, year to date, encumbrances, remaining,
  percent of budget, expected by pace (budget x elapsed share), variance, points versus pace,
  status (tolerance 10 points; "Over budget" when actual + encumbrance exceeds budget), the
  straight-line forecast (actual / elapsed), and for the monthly ledger the run-rate forecast
  (actual + average of the last three complete months x 12 x (1 - elapsed));
* the personnel pool (salary and wage categories combined) and whether it has at least 3 positions;
* the revenue mix (groups chosen by hand here, not by the tool's keyword lists);
* cost per service with allocation shares and an indirect rate;
* staffing: hours = volume x minutes / 60, productive hours = (paid - holiday - vacation - sick -
  training) x (1 - non-service share), FTE = hours / productive hours, rounded up to a step.

Writes expected_finance.json beside this script. With --check it writes nothing and exits 1 if
the file on disk differs.
"""
import csv
import json
import math
import sys
from datetime import date, timedelta
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
DATA = ROOT / "fixtures/synthetic/data"
OUT = HERE / "expected_finance.json"

PERSONNEL = {"Salaries and Wages", "Overtime"}
# Hand-assigned revenue groups (the tool assigns them by label words; this is the expectation).
GROUPS = {
    "Revolving Fund - Permits": "fees",
    "Fee for service": "fees",
    "State Public Health Excellence (PHE) Grant": "phe",
    "DPH Immunization Grant": "state_other",
    "CDC PHEP (federal)": "federal",
    "General Fund": "municipal",
    "Community Foundation grant": "other_grants",
    "Sale of publications": "other",
}
TOL = 10.0


def r4(x):
    return None if x is None else round(x + 0.0, 4)


def fiscal(as_of, start_month=7):
    start = date(as_of.year, start_month, 1)
    if start > as_of:
        start = date(as_of.year - 1, start_month, 1)
    nxt = date(start.year + 1, start_month, 1)
    days = (nxt - start).days
    elapsed_days = (as_of - start).days + 1
    return start, nxt, days, elapsed_days, elapsed_days / days


def num(s):
    s = (s or "").strip().replace("$", "").replace(",", "")
    return float(s) if s else None


def status(kind, budget, actual, enc, points):
    if budget == 0:
        return "No budget" if (actual > 0 and kind == "Expense") else "On pace"
    if kind == "Expense" and actual + enc > budget:
        return "Over budget"
    if points is not None and points > TOL:
        return "Ahead of pace"
    if points is not None and points < -TOL:
        return "Behind pace"
    return "On pace"


def line(kind, budget, actual, enc, elapsed, avg_monthly=None):
    e = enc if kind == "Expense" else 0.0
    pct = actual / budget * 100 if budget else None
    pts = pct - elapsed * 100 if pct is not None else None
    straight = actual / elapsed
    run = actual + avg_monthly * 12 * (1 - elapsed) if avg_monthly is not None else None
    sign = (lambda f: budget - f) if kind == "Expense" else (lambda f: f - budget)
    return {
        "budgeted": r4(budget),
        "actual": r4(actual),
        "encumbered": r4(enc) if kind == "Expense" else None,
        "remaining": r4(budget - actual - e),
        "percent_of_budget": r4(pct),
        "expected_by_pace": r4(budget * elapsed),
        "variance_vs_pace": r4(actual - budget * elapsed),
        "points_vs_pace": r4(pts),
        "status": status(kind, budget, actual, e, pts),
        "forecast_straight_line": r4(straight),
        "forecast_run_rate": r4(run),
        "projected_vs_budget_straight_line": r4(sign(straight)),
        "projected_vs_budget_run_rate": r4(sign(run)) if run is not None else None,
    }


def summary_case(fname, as_of, services=None, indirect=0.0):
    start, nxt, days, ed, elapsed = fiscal(as_of)
    rows = list(csv.DictReader(open(DATA / fname, newline="", encoding="utf-8")))
    exp, rev, pers = {}, {}, {"budget": 0.0, "actual": 0.0, "enc": 0.0, "positions": 0.0}
    mix = {}
    for r in rows:
        kind = r["Type"]
        b, a, e = num(r["Budgeted"]) or 0.0, num(r["YTD Actual"]) or 0.0, num(r["Encumbrances"]) or 0.0
        cat = r["Category"]
        if kind == "Expense":
            if cat in PERSONNEL:
                pers["budget"] += b
                pers["actual"] += a
                pers["enc"] += e
                pers["positions"] += num(r["Positions"]) or 0.0
            else:
                d = exp.setdefault(cat, [0.0, 0.0, 0.0])
                d[0] += b; d[1] += a; d[2] += e
        else:
            d = rev.setdefault(cat, [0.0, 0.0, 0.0])
            d[0] += b; d[1] += a
            m = mix.setdefault(GROUPS[r["Fund/Source"]], [0.0, 0.0])
            m[0] += b; m[1] += a
    shown = pers["positions"] >= 3
    out = {
        "as_of": as_of.isoformat(),
        "fiscal_year": {"start": start.isoformat(), "end": (nxt - timedelta(days=1)).isoformat(),
                        "label": "FY%d" % nxt.year, "days_in_year": days, "days_elapsed": ed,
                        "percent_elapsed": r4(elapsed * 100)},
        "personnel": {"positions": pers["positions"], "shown": shown},
    }
    ex = {c: line("Expense", *v, elapsed) for c, v in exp.items()}
    tot_b = sum(v[0] for v in exp.values()); tot_a = sum(v[1] for v in exp.values()); tot_e = sum(v[2] for v in exp.values())
    if pers["budget"] or pers["actual"]:
        if shown:
            ex["PERSONNEL"] = line("Expense", pers["budget"], pers["actual"], pers["enc"], elapsed)
            tot_b += pers["budget"]; tot_a += pers["actual"]; tot_e += pers["enc"]
    out["expenses"] = ex
    out["expense_totals"] = {**line("Expense", tot_b, tot_a, tot_e, elapsed),
                             "excludes_personnel": not shown}
    rv = {c: line("Revenue", *v, elapsed) for c, v in rev.items()}
    rb = sum(v[0] for v in rev.values()); ra = sum(v[1] for v in rev.values())
    out["revenue"] = rv
    out["revenue_totals"] = line("Revenue", rb, ra, 0.0, elapsed)
    out["mix"] = {g: {"budgeted": r4(v[0]), "actual": r4(v[1]),
                      "percent_of_budgeted": r4(v[0] / rb * 100), "percent_of_actual": r4(v[1] / ra * 100)}
                  for g, v in mix.items()}
    if shown:
        out["net"] = {"budgeted": r4(rb - tot_b), "actual_ytd": r4(ra - tot_a),
                      "revenue_covers_percent_of_expenses": r4(ra / tot_a * 100)}
    else:
        out["net"] = None
    if services:
        sv = {}
        for s in services:
            cost = budget = 0.0
            withheld = False
            for c, share in s["costs"]:
                if c == "Personnel":
                    if not shown:
                        withheld = True
                        continue
                    cost += pers["actual"] * share / 100
                    budget += pers["budget"] * share / 100
                else:
                    cost += exp[c][1] * share / 100
                    budget += exp[c][0] * share / 100
            if withheld:
                sv[s["name"]] = {"withheld": True}
                continue
            total = cost * (1 + indirect / 100)
            sv[s["name"]] = {
                "total_cost": r4(total),
                "cost_per_unit": r4(total / s["volume"]),
                "budgeted_cost_per_unit": r4(budget * (1 + indirect / 100) / s["planned"]) if s.get("planned") else None,
            }
        out["services"] = sv
    return out


def month_end(y, m):
    return (date(y + (m == 12), m % 12 + 1, 1) - timedelta(days=1))


def ledger_case(as_of, run_months=3):
    start, nxt, days, ed, elapsed = fiscal(as_of)
    rows = list(csv.DictReader(open(DATA / "health_dept_ledger_monthly.csv", newline="", encoding="utf-8")))
    lines = {}
    months_present = set()
    for r in rows:
        y, m = map(int, r["Month"].split("-"))
        key = (r["Type"], r["Category"], r["Fund/Source"], r["Account"])
        L = lines.setdefault(key, {"budget": None, "enc": None, "months": {}, "pos": 0.0})
        b = num(r["Budgeted"]); e = num(r["Encumbrances"]); a = num(r["Actual"])
        if b is not None:
            L["budget"] = b  # rows are in month order: the latest stands
        if e is not None:
            L["enc"] = e
        L["months"][(y, m)] = L["months"].get((y, m), 0.0) + (a or 0.0)
        L["pos"] = max(L["pos"], num(r["Positions"]) or 0.0)
        months_present.add((y, m))
    complete = sorted(m for m in months_present if month_end(*m) <= as_of)
    recent = complete[-run_months:]
    cats = {}
    for (kind, cat, src, acct), L in lines.items():
        pk = "PERSONNEL" if (kind == "Expense" and cat in PERSONNEL) else cat
        d = cats.setdefault((kind, pk), {"b": 0.0, "a": 0.0, "e": 0.0, "months": {}, "pos": 0.0})
        d["b"] += L["budget"] or 0.0
        d["a"] += sum(L["months"].values())
        d["e"] += (L["enc"] or 0.0) if kind == "Expense" else 0.0
        d["pos"] += L["pos"]
        for m, x in L["months"].items():
            d["months"][m] = d["months"].get(m, 0.0) + x
    out = {"as_of": as_of.isoformat(), "recent_months": ["%d-%02d" % m for m in recent],
           "percent_elapsed": r4(elapsed * 100), "expenses": {}, "revenue": {}}
    tot = {"Expense": [0, 0, 0, {}], "Revenue": [0, 0, 0, {}]}
    shown = cats[("Expense", "PERSONNEL")]["pos"] >= 3
    out["personnel"] = {"positions": cats[("Expense", "PERSONNEL")]["pos"], "shown": shown}
    for (kind, cat), d in sorted(cats.items()):
        avg = sum(d["months"].get(m, 0.0) for m in recent) / len(recent)
        key = "expenses" if kind == "Expense" else "revenue"
        out[key][cat] = line(kind, d["b"], d["a"], d["e"], elapsed, avg)
        if kind == "Expense" and cat == "PERSONNEL" and not shown:
            continue
        t = tot[kind]
        t[0] += d["b"]; t[1] += d["a"]; t[2] += d["e"]
        for m, x in d["months"].items():
            t[3][m] = t[3].get(m, 0.0) + x
    for kind, key in (("Expense", "expense_totals"), ("Revenue", "revenue_totals")):
        t = tot[kind]
        avg = sum(t[3].get(m, 0.0) for m in recent) / len(recent)
        out[key] = line(kind, t[0], t[1], t[2], elapsed, avg)
    return out


def staffing_case(services, scenarios, hours=None, current=None, population=None, clients=None, step=0.1):
    h = {"paid": 2080, "holiday": 88, "vacation": 120, "sick": 64, "training": 24, "non_service": 20}
    h.update(hours or {})
    available = h["paid"] - h["holiday"] - h["vacation"] - h["sick"] - h["training"]
    productive = available * (1 - h["non_service"] / 100)
    base = sum(v * m / 60 for _, v, m in services)

    def up(x):
        return math.ceil(round(x / step, 6)) * step

    def scen(name, growth, added):
        ah = sum(v * m / 60 for _, v, m in added)
        total = base * (1 + growth / 100) + ah
        fte = total / productive
        return {"name": name, "added_hours": r4(ah), "total_hours": r4(total), "fte_needed": r4(fte),
                "fte_rounded_up": r4(up(fte)),
                "gap_vs_current_fte": r4(up(fte) - current) if current else None,
                "residents_per_fte_needed": r4(population / fte) if population else None,
                "clients_per_fte_needed": r4(clients / fte) if clients else None}

    out = {"productive_hours_per_fte": r4(productive), "available_hours": r4(available),
           "baseline_hours": r4(base),
           "services": {n: {"hours": r4(v * m / 60), "percent_of_hours": r4(v * m / 60 / base * 100)} for n, v, m in services},
           "scenarios": [scen("Baseline", 0, [])] + [scen(n, g, a) for n, g, a in scenarios]}
    if current:
        out["current"] = {"residents_per_fte": r4(population / current) if population else None,
                          "clients_per_fte": r4(clients / current) if clients else None,
                          "baseline_workload_percent_of_capacity": r4(base / (current * productive) * 100)}
    return out


def build():
    services = [
        {"name": "Immunization clinics", "volume": 640,
         "costs": [("Vaccines and Medical Supplies", 100), ("Personnel", 25)], "planned": 1800},
        {"name": "Inspections", "volume": 210, "costs": [("Supplies", 50), ("Travel and Training", 100)]},
    ]
    staff = [("Home visits", 1200, 45), ("Clinic doses", 3000, 6), ("TB case management", 80, 120),
             ("Communicable disease investigations", 150, 60)]
    scen = [("10% growth", 10, []),
            ("Add diabetes prevention", 0, [("Diabetes prevention sessions", 120, 90)]),
            ("Growth plus added", 5, [("Diabetes prevention sessions", 120, 90), ("School vision screening", 900, 4)])]
    return {
        "summary": summary_case("health_dept_budget.csv", date(2026, 10, 31), services, indirect=10.0),
        "small": summary_case("health_dept_budget_small.csv", date(2026, 10, 31),
                              [{"name": "Immunization clinics", "volume": 300,
                                "costs": [("Supplies", 100), ("Personnel", 50)]}]),
        "ledger": ledger_case(date(2026, 10, 31)),
        "ledger_mid_month": ledger_case(date(2026, 10, 15), run_months=3),
        "staffing_default": staffing_case(staff, scen, current=1.5, population=24000, clients=340),
        "staffing_custom": staffing_case(
            staff, scen[:1],
            hours={"paid": 1950, "holiday": 80, "vacation": 100, "sick": 56, "training": 16, "non_service": 15},
            current=2.0, population=24000, clients=340, step=0.5),
    }


def main():
    text = json.dumps(build(), indent=1, sort_keys=True) + "\n"
    if "--check" in sys.argv:
        if not OUT.exists() or OUT.read_text() != text:
            print("expected_finance.json is out of date", file=sys.stderr)
            sys.exit(1)
        return
    OUT.write_text(text)


if __name__ == "__main__":
    main()
