#!/usr/bin/env python3
"""Reference values for M/M/c (Erlang C) queues: tests/fixtures/queueing.json.

Computed in exact rational arithmetic (fractions.Fraction) from the textbook
definition (Gross et al., Fundamentals of Queueing Theory, 4th ed., sec. 2.3):

    P0 = [ sum_{k<c} a^k/k! + a^c/c! * c/(c-a) ]^-1
    C  = a^c/c! * c/(c-a) * P0                  (probability of waiting)
    Lq = C * rho/(1-rho),  Wq = Lq/lambda,  W = Wq + 1/mu,  L = lambda*W

This is independent of the Rust code, which uses the Erlang B recursion.
Run: python3 -I queueing.py > ../queueing.json
"""
import json
from fractions import Fraction as F
from math import factorial


def mmc(c, lam, mu):
    a = lam / mu
    rho = a / c
    tail = a**c / factorial(c) * F(c) / (c - a)
    p0 = 1 / (sum(a**k / factorial(k) for k in range(c)) + tail)
    pw = tail * p0
    lq = pw * rho / (1 - rho)
    wq = lq / lam
    w = wq + 1 / mu
    return {"servers": c, "arrival_rate": float(lam), "service_rate": float(mu),
            "prob_wait": float(pw), "mean_queue": float(lq), "mean_wait": float(wq),
            "mean_time_in_system": float(w), "mean_in_system": float(lam * w),
            "p0": float(p0)}


cases = []
for c in [1, 2, 3, 4, 5, 8, 10, 15, 20, 30, 50]:
    for rho in [F(1, 10), F(3, 10), F(1, 2), F(7, 10), F(4, 5), F(9, 10), F(19, 20), F(99, 100)]:
        mu = F(12)  # 5-minute service
        lam = rho * c * mu
        cases.append(mmc(c, lam, mu))

# Published example (Hillier and Lieberman, Introduction to Operations Research, County
# Hospital emergency room: lambda = 2 patients/hour, mu = 3 patients/hour, s = 1 and s = 2):
# s=1: L = 2, Lq = 4/3, W = 1 h, Wq = 2/3 h; s=2: P0 = 1/2, L = 3/4, Lq = 1/12, W = 3/8 h,
# Wq = 1/24 h.
published = [
    {"source": "Hillier & Lieberman, County Hospital emergency room, s = 1",
     **mmc(1, F(2), F(3))},
    {"source": "Hillier & Lieberman, County Hospital emergency room, s = 2",
     **mmc(2, F(2), F(3))},
]
print(json.dumps({"generator": "queueing.py (exact rational arithmetic, Python fractions)",
                  "cases": cases, "published": published}, indent=1))
