#!/usr/bin/env python3
"""Independent computation of the golden economic evaluation cases (PLAN-10).

Usage: python3 -I economic_golden.py > ../economic_golden.json      (or --check)

Exact rational arithmetic (fractions.Fraction), written from the definitions rather than from the
Rust code:

* present value of year t (1-based) = amount / (1 + r)^(t - 1), or ^t when the first year is
  discounted;
* benefits = outcomes x value per outcome + other benefits, by year, discounted at the cost rate;
* net benefit = PV benefits - PV cost; BCR = PV benefits / PV cost; ROI = net benefit / PV cost;
* ICER = (PV cost - PV comparator cost) / (PV outcomes - PV comparator outcomes), with dominance
  (costs less and achieves more, or the reverse) reported instead of a ratio;
* break-even value per outcome found by bisection on the BCR (not by the closed form);
* one-way sensitivity: a stream's total is rescaled to the tested value.

The published section reproduces the benefit-cost results in Table 2 of CDC's MMWR 2024;73(31):682-685
("Health and Economic Benefits of Routine Childhood Immunizations in the Era of the Vaccines for
Children Program, United States, 1994-2023"): societal cost of immunization $268 billion, benefits in
societal costs $2,931 billion, net savings $2,663 billion, benefit-cost ratio 10.9; payer cost $240
billion, benefits in direct costs $780 billion, net savings $540 billion, benefit-cost ratio 3.3.
(Those benefits and costs were already discounted at 3 percent by the authors, so the tool is run
here with a 0 percent rate on the published totals.)
"""
import json
import sys
from fractions import Fraction as F
from pathlib import Path


def pv(stream, rate, first_discounted):
    shift = 1 if first_discounted else 0
    return sum((F(x) / (1 + rate) ** (i + shift) for i, x in enumerate(stream)), F(0))


def pad(v, n):
    return list(v) + [0] * (n - len(v))


def metrics(c):
    n = max(len(c["costs"]), len(c["outcomes"]), len(c["other_benefits"]),
            len(c.get("comp_costs") or []), len(c.get("comp_outcomes") or []), 1)
    rate, orate, fy = F(str(c["rate"])), F(str(c["outcome_rate"])), c["first_year_discounted"]
    cost = [F(str(x)) for x in pad(c["costs"], n)]
    out = [F(str(x)) for x in pad(c["outcomes"], n)]
    oth = [F(str(x)) for x in pad(c["other_benefits"], n)]
    bpo = F(str(c["benefit_per_outcome"])) if c["benefit_per_outcome"] is not None else None
    pv_cost, pv_out = pv(cost, rate, fy), pv(out, orate, fy)
    m = {"pv_cost": pv_cost, "pv_outcomes": pv_out, "total_cost": sum(cost, F(0)), "total_outcomes": sum(out, F(0))}
    m["cost_per_outcome_pv"] = pv_cost / pv_out if pv_out > 0 else None
    parts = F(str(c["participants"])) if c["participants"] is not None else None
    m["cost_per_participant"] = m["total_cost"] / parts if parts else None
    has_b = bpo is not None or any(x != 0 for x in oth)
    if has_b:
        ben = [(out[i] * (bpo or 0)) + oth[i] for i in range(n)]
        m["pv_benefits"] = pv(ben, rate, fy)
        m["net_benefit"] = m["pv_benefits"] - pv_cost
        m["bcr"] = m["pv_benefits"] / pv_cost
        m["roi"] = m["net_benefit"] / pv_cost
        cum, payback = F(0), None
        for i in range(n):
            cum += (ben[i] - cost[i]) / (1 + rate) ** (i + (1 if fy else 0))
            if cum >= 0:
                payback = i + 1
                break
        m["payback_year"] = payback
    # Break-even value per outcome by bisection on the BCR (independent of the closed form).
    if pv_out > 0:
        lo, hi = F(0), F(10) ** 9
        for _ in range(300):
            mid = (lo + hi) / 2
            ben = [out[i] * mid + oth[i] for i in range(n)]
            if pv(ben, rate, fy) >= pv_cost:
                hi = mid
            else:
                lo = mid
        m["break_even_value"] = max(hi, F(0)) if pv(oth, rate, fy) < pv_cost else F(0)
    if c.get("comp_costs") is not None:
        cc = pv([F(str(x)) for x in pad(c["comp_costs"], n)], rate, fy)
        ce = pv([F(str(x)) for x in pad(c["comp_outcomes"], n)], orate, fy)
        dc, de = pv_cost - cc, pv_out - ce
        m["delta_cost"], m["delta_outcomes"] = dc, de
        if de == 0:
            cls = "no_difference"
        elif de > 0:
            cls = "dominant" if dc <= 0 else "more_costly_more_effective"
        else:
            cls = "dominated" if dc >= 0 else "less_costly_less_effective"
        m["classification"] = cls
        m["icer"] = dc / de if cls in ("more_costly_more_effective", "less_costly_less_effective") else None
        if c.get("willingness_to_pay") is not None:
            m["nmb"] = F(str(c["willingness_to_pay"])) * de - dc
    elif c.get("willingness_to_pay") is not None:
        m["nmb"] = F(str(c["willingness_to_pay"])) * pv_out - pv_cost
    return m


def scaled(stream, new_total):
    """The stream rescaled so its total is `new_total`."""
    t = sum(F(str(x)) for x in stream)
    return [F(str(x)) * F(str(new_total)) / t for x in stream]


def bcr_exact(costs, outcomes, other, bpo, rate, first_discounted):
    n = max(len(costs), len(outcomes), len(other), 1)
    cost = [F(x) for x in pad(costs, n)]
    out = [F(x) for x in pad(outcomes, n)]
    oth = [F(x) for x in pad(other, n)]
    ben = [out[i] * F(str(bpo or 0)) + oth[i] for i in range(n)]
    return pv(ben, rate, first_discounted) / pv(cost, rate, first_discounted)


def sens_bcr(c, param, value):
    """BCR when one number is set to `value` and the others stay at their base values."""
    costs, outcomes, other = [F(str(x)) for x in c["costs"]], [F(str(x)) for x in c["outcomes"]], [F(str(x)) for x in c["other_benefits"]]
    bpo, rate = c["benefit_per_outcome"], F(str(c["rate"]))
    if param == "program_cost":
        costs = scaled(c["costs"], value)
    elif param == "benefit_per_outcome":
        bpo = value
    elif param == "outcomes":
        outcomes = scaled(c["outcomes"], value)
    elif param == "discount_rate":
        rate = F(str(value)) / 100
    return bcr_exact(costs, outcomes, other, bpo, rate, c["first_year_discounted"])


def case(name, **kw):
    base = dict(costs=[], outcomes=[], other_benefits=[], benefit_per_outcome=None, participants=None,
                comp_costs=None, comp_outcomes=None, rate=0.03, outcome_rate=0.03,
                first_year_discounted=False, willingness_to_pay=None)
    base.update(kw)
    base["name"] = name
    return base


CASES = [
    case("level_three_years_with_benefits", costs=[100000, 100000, 100000], outcomes=[40, 40, 40],
         benefit_per_outcome=3500, participants=600),
    case("first_year_also_discounted", costs=[50000, 20000, 20000, 20000], outcomes=[0, 30, 30, 30],
         benefit_per_outcome=2500, participants=250, first_year_discounted=True),
    case("zero_discount_rate", costs=[75000], outcomes=[25], other_benefits=[30000], rate=0, outcome_rate=0,
         benefit_per_outcome=1200, participants=300),
    case("other_savings_only_five_years", costs=[80000, 30000, 30000, 30000, 30000],
         other_benefits=[0, 45000, 60000, 60000, 60000], participants=420),
    case("icer_more_costly_more_effective", costs=[120000, 120000], outcomes=[30, 30],
         comp_costs=[40000, 40000], comp_outcomes=[10, 10], willingness_to_pay=5000, participants=400),
    case("icer_dominant", costs=[60000], outcomes=[35], comp_costs=[90000], comp_outcomes=[20], participants=200),
    case("icer_dominated", costs=[90000], outcomes=[20], comp_costs=[60000], comp_outcomes=[35], participants=200),
    case("icer_less_costly_less_effective", costs=[50000, 50000], outcomes=[8, 8],
         comp_costs=[90000, 90000], comp_outcomes=[15, 15], participants=150),
    case("icer_no_difference", costs=[50000], outcomes=[10], comp_costs=[40000], comp_outcomes=[10]),
    case("cost_per_outcome_distinct_rates", costs=[100000, 100000, 100000], outcomes=[10, 10, 10],
         rate=0.03, outcome_rate=0.05, participants=90),
]

SENS = {
    "level_three_years_with_benefits": [("program_cost", 240000, 360000), ("benefit_per_outcome", 2800, 4200),
                                        ("outcomes", 96, 144), ("discount_rate", 0, 7)],
}

MMWR = [
    dict(name="societal", cost=268, benefits=2931, published_bcr=10.9, published_net=2663),
    dict(name="payer", cost=240, benefits=780, published_bcr=3.3, published_net=540),
]


def num(x):
    return None if x is None else format(float(x), ".15g")


def main():
    out = {"generated_by": "economic_golden.py", "cases": [], "published": {
        "source": "CDC MMWR 2024;73(31):682-685, Table 2 (https://www.cdc.gov/mmwr/volumes/73/wr/mm7331a2.htm)",
        "note": "Totals in billions of dollars, already discounted at 3 percent by the authors.",
        "cases": []}}
    for c in CASES:
        m = metrics(c)
        exp = {k: (v if isinstance(v, (str, int)) or v is None else num(v)) for k, v in m.items()}
        entry = {**c, "expected": exp}
        if c["name"] in SENS:
            rows = []
            for p, lo, hi in SENS[c["name"]]:
                rows.append({"parameter": p, "low": lo, "high": hi,
                             "bcr_low": num(sens_bcr(c, p, lo)), "bcr_high": num(sens_bcr(c, p, hi))})
            entry["sensitivity_bcr"] = rows
        out["cases"].append(entry)
    for p in MMWR:
        out["published"]["cases"].append({**p, "computed_bcr": num(F(p["benefits"], p["cost"])),
                                          "computed_net": p["benefits"] - p["cost"],
                                          "computed_roi": num(F(p["benefits"] - p["cost"], p["cost"]))})
    text = json.dumps(out, indent=1) + "\n"
    target = Path(__file__).resolve().parent.parent / "economic_golden.json"
    if "--check" in sys.argv:
        if target.read_text() != text:
            sys.exit("economic_golden.json is out of date: rerun economic_golden.py")
        print("economic_golden.json is current")
    else:
        sys.stdout.write(text)


if __name__ == "__main__":
    main()
