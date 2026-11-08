"""Add Breslow-Day (Tarone) and Mantel-Haenszel risk ratios to two_by_two.json.

Run after two_by_two.R:  python two_by_two_statsmodels.py two_by_two.json
"""
import json
import sys

import numpy as np
import statsmodels
from statsmodels.stats.contingency_tables import StratifiedTable

path = sys.argv[1]
data = json.load(open(path))
for s in data["stratified"]:
    tables = [np.array([[t[0], t[1]], [t[2], t[3]]], dtype=float) for t in s["strata"]]
    st = StratifiedTable(tables)
    bd = st.test_equal_odds(adjust=True)
    s["breslow_day_tarone_stat"] = float(bd.statistic)
    s["breslow_day_tarone_p"] = float(bd.pvalue)
    s["mh_rr"] = float(st.riskratio_pooled)
data["statsmodels_version"] = statsmodels.__version__
data["note"] += " SciPy adds precise conditional-MLE, Fisher, and mid-p exact limits."
json.dump(data, open(path, "w"), indent=2)

# SciPy's conditional odds ratio (brentq with tight tolerance) is more precise
# than R's fisher.test, whose uniroot tolerance is about 1e-4.
import scipy
from scipy.stats.contingency import odds_ratio

for t in data["tables"]:
    res = odds_ratio([[t["a"], t["b"]], [t["c"], t["d"]]], kind="conditional")
    ci = res.confidence_interval(0.95)
    t["scipy_or_mle"] = float(res.statistic) if np.isfinite(res.statistic) else None
    t["scipy_or_exact_lower"] = float(ci.low)
    t["scipy_or_exact_upper"] = float(ci.high) if np.isfinite(ci.high) else None
data["scipy_version"] = scipy.__version__
json.dump(data, open(path, "w"), indent=2)

# Mid-p exact limits with SciPy's Fisher noncentral hypergeometric and brentq
# (epitools::oddsratio.midp uses uniroot, which is only accurate to ~1e-4).
from scipy.optimize import brentq
from scipy.stats import nchypergeom_fisher


def midp_limits(a, b, c, d, alpha=0.05):
    M, n, N = a + b + c + d, a + c, a + b  # total, cases, exposed
    lo, hi = max(0, N - (M - n)), min(N, n)

    def lower_tail(lp):  # P(X < a) + P(a)/2
        dist = nchypergeom_fisher(M, n, N, np.exp(lp))
        return dist.cdf(a - 1) + 0.5 * dist.pmf(a)

    def upper_tail(lp):  # P(X > a) + P(a)/2
        dist = nchypergeom_fisher(M, n, N, np.exp(lp))
        return dist.sf(a) + 0.5 * dist.pmf(a)

    low = 0.0 if a == lo else float(np.exp(brentq(lambda lp: upper_tail(lp) - alpha / 2, -30, 30, xtol=1e-14)))
    high = None if a == hi else float(np.exp(brentq(lambda lp: lower_tail(lp) - alpha / 2, -30, 30, xtol=1e-14)))
    return low, high


for t in data["tables"]:
    low, high = midp_limits(t["a"], t["b"], t["c"], t["d"])
    t["scipy_midp_lower"] = low
    t["scipy_midp_upper"] = high
json.dump(data, open(path, "w"), indent=2)
