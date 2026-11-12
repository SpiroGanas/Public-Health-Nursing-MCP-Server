#!/usr/bin/env python3
"""Independent computation of the golden budget cases (build_program_budget, PLAN-09).

Usage: python3 -I budget_golden.py > ../budget_golden.json      (or --check to compare)

This program re-implements the budget rules from the written specification, with exact decimal
arithmetic (no shared code with the Rust crate):

* line-year amount = ROUND(quantity x unit cost x (1 + escalation)^(year - 1), 2) when the line is
  active in that year and escalates (else without the growth factor); personnel quantity is the
  fraction of time and unit cost is the annual salary;
* fringe = ROUND(personnel amount x fringe rate, 2);
* MTDC base = amount + fringe for "Included" lines, 0 for "Excluded", and for a "Subaward" the
  part of the cumulative amount that falls within the first cap dollars (2 CFR 200.1: the first
  $50,000 of each subaward, regardless of the period of performance);
* indirect = ROUND(rate x base, 2) per year; the cost share block is charged only when the case
  says so; the match check compares the required percent of the federal request or of the total
  project with the cost share total.

Inputs for the cases are written into the JSON with the expected results so the Rust test and the
LibreOffice recalculation check (check_budget_workbook.py) use the same numbers.
"""
import json
import sys
from decimal import Decimal as D, ROUND_HALF_UP
from pathlib import Path

CENT = D("0.01")


def r2(x):
    return x.quantize(CENT, rounding=ROUND_HALF_UP)


def line(category, item, quantity, unit_cost, **kw):
    d = dict(category=category, item=item, funding="federal", quantity=quantity, unit_cost=unit_cost,
             fringe=None, escalate=(category != "equipment"), first_year=1, last_year=None,
             treatment="Included")
    d.update(kw)
    if category == "equipment":
        d["treatment"] = "Excluded"
    return d


CASES = [
    dict(
        name="one_year_basic", years=1, salary_escalation=0, other_escalation=0, fringe=0.25,
        indirect_rate=0.15, indirect_base="mtdc", indirect_on_cost_share=False, subaward_cap=50000,
        required_match=None,
        lines=[
            line("personnel", "Public health nurse", 0.5, 80000),
            line("equipment", "Vaccine refrigerator", 1, 12000),
            line("supplies", "Clinic supplies", 1, 3000),
        ],
    ),
    dict(
        name="three_year_escalation_subaward_cost_share", years=3, salary_escalation=0.03,
        other_escalation=0.02, fringe=0.28, indirect_rate=0.15, indirect_base="mtdc",
        indirect_on_cost_share=False, subaward_cap=50000, required_match=[20, "total_project"],
        lines=[
            line("personnel", "Program coordinator", 1.0, 61234.56),
            line("personnel", "Public health nurse", 0.2, 95000, fringe=0.30),
            line("travel", "Local mileage", 4, 350),
            line("equipment", "Laptop cart", 1, 10500, last_year=1),
            line("contractual", "Community partner subaward", 1, 30000, treatment="Subaward"),
            line("contractual", "Evaluation contractor", 1, 12000, first_year=2),
            line("other", "Participant incentives", 1, 5000, treatment="Excluded"),
            line("personnel", "Nurse supervisor (match)", 0.1, 100000, funding="cost_share"),
            line("supplies", "Donated space supplies", 12, 85.5, funding="cost_share"),
        ],
    ),
    dict(
        name="negotiated_total_direct_cost_share_indirect", years=2, salary_escalation=0.0,
        other_escalation=0.015, fringe=0.31, indirect_rate=0.125, indirect_base="total_direct",
        indirect_on_cost_share=True, subaward_cap=50000, required_match=[25, "federal_request"],
        lines=[
            line("personnel", "Community health worker", 0.75, 48000.55, last_year=2),
            line("equipment", "Blood pressure monitors", 20, 450, last_year=1),
            line("supplies", "Teaching materials", 2, 1111.11, last_year=2),
            line("other", "Interpreter services", 15, 62.5, last_year=2),
            line("supplies", "In-kind printing", 1, 2500, funding="cost_share", last_year=2),
        ],
    ),
    dict(
        name="negotiated_salaries_base", years=2, salary_escalation=0.025, other_escalation=0.0,
        fringe=0.22, indirect_rate=0.40, indirect_base="salaries_and_wages",
        indirect_on_cost_share=False, subaward_cap=50000, required_match=None,
        lines=[
            line("personnel", "Epidemiologist", 0.4, 90125, last_year=2),
            line("travel", "Conference travel", 1, 1800, last_year=2),
            line("contractual", "Data vendor", 1, 7000, last_year=2),
        ],
    ),
    dict(
        name="negotiated_salaries_and_fringe_base", years=2, salary_escalation=0.03,
        other_escalation=0.0, fringe=0.27, indirect_rate=0.30, indirect_base="salaries_and_fringe",
        indirect_on_cost_share=False, subaward_cap=50000, required_match=None,
        lines=[
            line("personnel", "School nurse leader", 0.5, 70000.5, last_year=2),
            line("supplies", "Screening supplies", 1, 4000, last_year=2),
        ],
    ),
]

CATS = ["personnel", "travel", "equipment", "supplies", "contractual", "other"]


def compute(case):
    n = case["years"]
    rows = []
    for ln in case["lines"]:
        last = ln["last_year"] or n
        esc = D(str(case["salary_escalation"] if ln["category"] == "personnel" else case["other_escalation"]))
        q, u = D(str(ln["quantity"])), D(str(ln["unit_cost"]))
        fr = D(str(ln["fringe"] if ln["fringe"] is not None else case["fringe"]))
        amt, frg, mt = [], [], []
        cum_prev = D(0)
        for y in range(1, n + 1):
            active = ln["first_year"] <= y <= last
            a = D(0)
            if active:
                f = (1 + esc) ** (y - 1) if ln["escalate"] else D(1)
                a = r2(q * u * f)
            fg = r2(a * fr) if ln["category"] == "personnel" else D(0)
            cum = cum_prev + a
            cap = D(str(case["subaward_cap"]))
            if ln["treatment"] == "Excluded":
                m = D(0)
            elif ln["treatment"] == "Included":
                m = a + fg
            else:
                m = min(cum, cap) - min(cum_prev, cap)
            cum_prev = cum
            amt.append(a), frg.append(fg), mt.append(m)
        rows.append((ln, amt, frg, mt))

    def block(funding):
        b = {c: [D(0)] * n for c in CATS}
        b["fringe"], b["mtdc"] = [D(0)] * n, [D(0)] * n
        for ln, amt, frg, mt in rows:
            if ln["funding"] != funding:
                continue
            b[ln["category"]] = [x + y for x, y in zip(b[ln["category"]], amt)]
            b["fringe"] = [x + y for x, y in zip(b["fringe"], frg)]
            b["mtdc"] = [x + y for x, y in zip(b["mtdc"], mt)]
        direct = [sum((b[c][k] for c in CATS), D(0)) + b["fringe"][k] for k in range(n)]
        base_kind = case["indirect_base"]
        base = []
        for k in range(n):
            if base_kind == "mtdc":
                base.append(b["mtdc"][k])
            elif base_kind == "total_direct":
                base.append(direct[k])
            elif base_kind == "salaries_and_wages":
                base.append(b["personnel"][k])
            else:
                base.append(b["personnel"][k] + b["fringe"][k])
        charge = funding == "federal" or case["indirect_on_cost_share"]
        rate = D(str(case["indirect_rate"]))
        ind = [r2(rate * base[k]) if charge else D(0) for k in range(n)]
        out = {c: [str(x) for x in b[c]] for c in CATS}
        out["fringe"] = [str(x) for x in b["fringe"]]
        out["direct"] = [str(x) for x in direct]
        out["indirect_base"] = [str(x) for x in base]
        out["indirect"] = [str(x) for x in ind]
        out["total"] = [str(direct[k] + ind[k]) for k in range(n)]
        return out

    fed, cs = block("federal"), block("cost_share")
    exp = {"federal": fed, "cost_share": cs}
    if case["required_match"]:
        pct, basis = case["required_match"]
        fed_t = sum((D(x) for x in fed["total"]), D(0))
        cs_t = sum((D(x) for x in cs["total"]), D(0))
        required = D(str(pct)) / 100 * (fed_t if basis == "federal_request" else fed_t + cs_t)
        exp["match"] = {"required": str(r2(required)), "budgeted": str(cs_t),
                        "shortfall": str(r2(max(required - cs_t, D(0))))}
    return exp


def main():
    out = {"generated_by": "budget_golden.py", "cases": []}
    for c in CASES:
        out["cases"].append({**c, "expected": compute(c)})
    text = json.dumps(out, indent=1) + "\n"
    target = Path(__file__).resolve().parent.parent / "budget_golden.json"
    if "--check" in sys.argv:
        if target.read_text() != text:
            sys.exit("budget_golden.json is out of date: rerun budget_golden.py")
        print("budget_golden.json is current")
    else:
        sys.stdout.write(text)


if __name__ == "__main__":
    main()
