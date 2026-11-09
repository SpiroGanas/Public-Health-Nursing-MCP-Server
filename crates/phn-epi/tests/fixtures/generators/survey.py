#!/usr/bin/env python3
"""Reference values for CASPER weighted proportions: tests/fixtures/survey.json.

A synthetic CASPER (30 clusters, up to 7 interviews each, a frame of 43,214
households as in the CDC CASPER Toolkit's Figure 4) with a three-category
answer and some missing answers. Weights follow the toolkit's formula
(households in frame / (interviews in cluster x 30)). Estimates come from the
samplics package (TaylorEstimator, PopParam.prop, psu = cluster, no
strata, no finite population correction), version 0.6.0.

Run in a virtual environment with samplics installed:
    python -I survey.py > ../survey.json
"""
import json
import math

import numpy as np
from samplics.estimation import TaylorEstimator
from samplics.utils.types import PopParam

FRAME = 43214
labels = ["Single family home", "Multiple unit", "Other"]
# Interviews completed per cluster (cluster 3 was selected twice: 12 of 14 -> 6 each).
completed = [7, 7, 6, 7, 5, 7, 7, 4, 7, 7, 6, 7, 7, 7, 3, 7, 7, 7, 6, 7,
             7, 7, 5, 7, 7, 7, 7, 6, 7, 2]
cluster, weight, answer = [], [], []
state = 12345
for c, k in enumerate(completed, start=1):
    for i in range(k):
        # Deterministic pseudo-random answers (linear congruential generator).
        state = (1103515245 * state + 12345) % 2**31
        u = state / 2**31
        a = 0 if u < 0.62 - 0.01 * (c % 7) else (1 if u < 0.96 else 2)
        if (c * 7 + i) % 23 == 0:
            a = None  # missing answer
        cluster.append(c)
        weight.append(FRAME / (k * 30))
        answer.append(a)

y = np.array([labels[a] if a is not None else np.nan for a in answer], dtype=object)
mask = np.array([a is not None for a in answer])
est = TaylorEstimator(PopParam.prop)
est.estimate(y=y[mask], samp_weight=np.array(weight)[mask], psu=np.array(cluster)[mask])
estimates = []
for k, lab in enumerate(labels):
    idx = [i for i, a in enumerate(answer) if a == k]
    estimates.append({
        "category": lab,
        "n": len(idx),
        "weighted_count": sum(weight[i] for i in idx),
        "proportion": float(est.point_est[lab]),
        "stderror": float(est.stderror[lab]),
    })
print(json.dumps({"generator": "survey.py (samplics 0.6.0 TaylorEstimator)", "frame": FRAME,
                  "cluster": cluster, "weight": weight, "answer": answer, "labels": labels,
                  "estimates": estimates}, indent=1))
