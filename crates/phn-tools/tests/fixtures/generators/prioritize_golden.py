#!/usr/bin/env python3
"""Generate crates/phn-tools/tests/fixtures/prioritize_golden.json (golden values for EQ-09).

Independent of the Rust code: the Hanlon basic priority rating is computed in exact rational
arithmetic straight from the published equation (Hanlon and Pickett 1984, as quoted in Rev Panam
Salud Publica 2019;43:e61, "Original Hanlon method and its revisions"):

    BPR = ((A + B) x C / 3) x D

with A 0-10, B 0-20, C 0-10, and D = 1 only when all five PEARL factors are yes. Ranks are
competition ranks (1, 2, 2, 4). Nominal group points: with N ranks per participant, rank r earns
N + 1 - r points per participant who gave that rank.

Usage (from the repository root):
    python3 crates/phn-tools/tests/fixtures/generators/prioritize_golden.py > crates/phn-tools/tests/fixtures/prioritize_golden.json
"""
import json
import random
from fractions import Fraction

rng = random.Random(15)


def bpr(a, b, c):
    return (Fraction(a) + Fraction(b)) * Fraction(c) / 3


def competition_ranks(scores):
    order = sorted(range(len(scores)), key=lambda i: -scores[i])
    ranks = [0] * len(scores)
    rank = 0
    for pos, i in enumerate(order):
        if pos == 0 or scores[order[pos - 1]] != scores[i]:
            rank = pos + 1
        ranks[i] = rank
    return ranks


def num(x):
    return float(x)


hanlon = []
for a, b, c in [
    (8, 14, 6), (10, 20, 10), (0, 0, 10), (6.5, 12, 4), (5, 10, 5), (1, 1, 1),
    (9.5, 3, 7.25), (2, 19.5, 0.5), (10, 0, 10), (3, 17, 7),
]:
    v = bpr(Fraction(str(a)), Fraction(str(b)), Fraction(str(c)))
    hanlon.append({"a": a, "b": b, "c": c, "bpr": num(v), "bpr_exact": f"{v.numerator}/{v.denominator}"})

rankings = []
for mode in ["multiply", "prescreen", "off"]:
    for case in range(4):
        n = rng.randint(3, 7)
        issues = []
        for k in range(n):
            a = rng.randint(0, 10)
            b = rng.randint(0, 20)
            c = rng.randint(0, 10)
            pearl = [rng.random() > 0.2 for _ in range(5)]
            issues.append({"name": f"Issue {case}-{k}", "a": a, "b": b, "c": c, "pearl": pearl})
        # Force one tie in the first case of each mode.
        if case == 0 and n >= 3:
            issues[1].update(a=issues[0]["a"], b=issues[0]["b"], c=issues[0]["c"], pearl=[True] * 5)
            issues[0]["pearl"] = [True] * 5
        expected = []
        scored = []
        for i in issues:
            passes = all(i["pearl"])
            base = bpr(i["a"], i["b"], i["c"])
            if mode == "multiply":
                score, status = (base if passes else Fraction(0)), "ranked"
            elif mode == "prescreen":
                score, status = (base, "ranked") if passes else (None, "screened_out")
            else:
                score, status = base, "ranked"
            expected.append({"name": i["name"], "score": None if score is None else num(score), "status": status})
            if score is not None:
                scored.append((i["name"], score))
        ranks = competition_ranks([s for _, s in scored])
        rank_of = {name: r for (name, _), r in zip(scored, ranks)}
        for e in expected:
            e["rank"] = rank_of.get(e["name"])
        rankings.append({"mode": mode, "issues": issues, "expected": expected})

nominal = []
for case in range(5):
    participants = rng.randint(6, 15)
    n = rng.randint(2, 5)
    k = rng.randint(3, 6)
    # Assign each participant's ranked choices without replacement.
    counts = {f"Topic {j}": [0] * n for j in range(k)}
    for _ in range(participants):
        picks = rng.sample(list(counts), n)
        for r, topic in enumerate(picks):
            counts[topic][r] += 1
    points = {t: sum((n - r) * c for r, c in enumerate(cs)) for t, cs in counts.items()}
    names = list(counts)
    ranks = competition_ranks([points[t] for t in names])
    nominal.append(
        {
            "participants": participants,
            "ranks_per_participant": n,
            "counts": counts,
            "points": points,
            "ranks": dict(zip(names, ranks)),
        }
    )

print(
    json.dumps(
        {
            "description": "Golden values for EQ-09 prioritize_health_issues, computed in exact rational arithmetic by crates/phn-tools/tests/fixtures/generators/prioritize_golden.py from the equation BPR = ((A + B) x C / 3) x D (Hanlon and Pickett 1984, as quoted in Rev Panam Salud Publica 2019;43:e61).",
            "hanlon": hanlon,
            "rankings": rankings,
            "nominal": nominal,
        },
        indent=1,
    )
)
