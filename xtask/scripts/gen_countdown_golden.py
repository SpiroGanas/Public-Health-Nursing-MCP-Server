#!/usr/bin/env python3
"""Generate crates/phn-tools/tests/fixtures/countdown_golden.json: reference values for the comment
period countdown of monitor_federal_rules (POL-03), computed with Python's datetime arithmetic
(an independent implementation of calendar-day differences and weekday names).

Usage: python3 -I xtask/scripts/gen_countdown_golden.py [--check]
"""
import datetime
import json
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parents[2] / "crates/phn-tools/tests/fixtures/countdown_golden.json"

PAIRS = [
    ("2026-11-13", d) for d in ["2026-11-13", "2026-11-14", "2026-11-20", "2026-11-21", "2026-12-14", "2026-12-31", "2027-01-01", "2027-02-28", "2027-03-01", "2026-11-12", "2026-11-01", "2026-10-15", "2025-12-31"]
] + [
    ("2027-12-30", "2028-02-29"),
    ("2028-02-28", "2028-02-29"),
    ("2028-02-29", "2028-03-01"),
    ("2028-03-01", "2028-02-28"),
    ("2026-03-07", "2026-03-09"),
    ("2026-03-08", "2026-03-09"),
    ("2026-11-01", "2026-11-02"),
    ("2026-12-31", "2027-01-01"),
    ("2025-12-31", "2026-01-01"),
    ("2099-12-31", "2100-01-01"),
    ("1999-12-31", "2000-02-29"),
    ("2000-02-29", "2000-03-01"),
    ("2100-02-28", "2100-03-01"),
    ("2026-01-31", "2026-02-28"),
    ("2026-05-31", "2026-06-30"),
    ("2026-07-04", "2026-09-07"),
    ("2026-11-26", "2026-12-25"),
    ("2026-02-01", "2026-11-13"),
    ("2026-11-13", "2027-11-13"),
    ("2026-11-13", "2028-11-13"),
    ("2024-02-29", "2025-02-28"),
]


def cases():
    out = []
    for t, c in PAIRS:
        td = datetime.date.fromisoformat(t)
        cd = datetime.date.fromisoformat(c)
        out.append({"today": t, "close": c, "days": (cd - td).days, "weekday": cd.strftime("%A")})
    return out


def main():
    text = json.dumps(cases(), indent=1) + "\n"
    if "--check" in sys.argv:
        sys.exit(0 if OUT.read_text() == text else 1)
    OUT.write_text(text)
    print(f"wrote {OUT} ({len(PAIRS)} cases)")


if __name__ == "__main__":
    main()
