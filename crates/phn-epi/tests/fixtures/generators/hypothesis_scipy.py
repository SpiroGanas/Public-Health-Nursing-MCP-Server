"""Add the Brown-Forsythe test (scipy.stats.levene, center='median') to hypothesis.json."""
import json
import sys

import scipy
from scipy import stats

path = sys.argv[1]
d = json.load(open(path))
g = [d["data"]["g1"], d["data"]["g2"], d["data"]["g3"]]
r = stats.levene(*g, center="median")
d["brown_forsythe"] = {"stat": float(r.statistic), "p": float(r.pvalue)}
d["scipy_version"] = scipy.__version__
json.dump(d, open(path, "w"), indent=2)
