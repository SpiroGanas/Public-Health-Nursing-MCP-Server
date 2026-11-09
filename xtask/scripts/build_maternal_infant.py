"""Build snapshots/ma_maternal_infant.csv for maternal_infant_health_profile (MCH-06).

usage: build_maternal_infant.py BIRTH_2023_DOCX DEATH_2022_DOCX PRAMS_2019_2021_DOCX
                                US_2024Q4_JSON US_IM_2023Q4_JSON OUT_CSV

Inputs (downloaded with plain curl):
  https://www.mass.gov/doc/2023-birth-report/download                 (DOCX, modified 2026-08-19)
  https://www.mass.gov/doc/2022-death-report-docx/download            (DOCX, modified 2025-01-08)
  https://www.mass.gov/doc/2019-2021-mass-prams-report-docx/download   (DOCX)
  https://data.cdc.gov/resource/76vv-a7x8.json?$where=(indicator='Total preterm (<37 weeks)'
      or indicator='Low birthweight (Less than 2,500g)') and year_and_quarter='2024 Q4'
  https://data.cdc.gov/resource/jqwm-z2g9.json?$where=indicator='Infant mortality'
      and year_and_quarter='2023 Q4'

Rows are long format: geo_type, geography, indicator, group, period, numerator, denominator,
value, lower, upper, unit, source. Values are copied as published; for towns and counties the
percentages are computed here from DPH's counts (numerator / resident births). DPH suppresses
counts of 1-4 ("--"); those rows keep numerator "<5" and no value.
"""
import csv, json, re, sys, zipfile

BIRTH, DEATH, PRAMS, US_BW, US_IM, OUT = sys.argv[1:7]


def tables(path):
    x = zipfile.ZipFile(path).read("word/document.xml").decode("utf-8")
    out = []
    for t in re.findall(r"<w:tbl>.*?</w:tbl>", x, re.S):
        rows = []
        for tr in re.findall(r"<w:tr[ >].*?</w:tr>", t, re.S):
            rows.append(["".join(re.findall(r"<w:t[^>]*>([^<]*)</w:t>", tc)).strip()
                         for tc in re.findall(r"<w:tc>.*?</w:tc>", tr, re.S)])
        out.append(rows)
    return out


def num(s):
    s = (s or "").replace(",", "").strip()
    try:
        return float(s)
    except ValueError:
        return None


def find(ts, title):
    for t in ts:
        if t and t[0] and title in t[0][0]:
            return t
    raise SystemExit(f"table not found: {title}")


rows = []


def add(geo_type, geo, ind, group, period, n, d, value, lo, hi, unit, src):
    rows.append([geo_type, geo, ind, group, period,
                 "" if n is None else (n if isinstance(n, str) else f"{n:.0f}"),
                 "" if d is None else f"{d:.0f}",
                 "" if value is None else f"{value:.2f}".rstrip("0").rstrip("."),
                 "" if lo is None else lo, "" if hi is None else hi, unit, src])


B = tables(BIRTH)
# Table 17: resident births by municipality (and Table 17 county version).
for t, gtype in [(find(B, "Table 17. Birth Characteristics: Occurrence and Resident Births"), "town")]:
    for r in t[3:]:
        if len(r) < 6 or not r[0] or r[0].startswith("1."):
            continue
        name = "Massachusetts" if r[0] == "STATE TOTAL" else r[0]
        typ = "state" if r[0] == "STATE TOTAL" else gtype
        births = num(r[2])
        add(typ, name, "resident_births", "Total", "2023", births, None, births, None, None, "births", "ma-births-2023")
        for col, ind in [(3, "low_birthweight_pct"), (4, "preterm_pct")]:
            c = r[col]
            if c.startswith("--"):
                add(typ, name, ind, "Total", "2023", "<5", births, None, None, None, "% of births", "ma-births-2023")
            else:
                n = num(c)
                v = 100 * n / births if births else None
                add(typ, name, ind, "Total", "2023", n, births, v, None, None, "% of births", "ma-births-2023")
        c = r[5]
        add(typ, name, "teen_births", "Total", "2023", "<5" if c.startswith("--") else num(c), None,
            None if c.startswith("--") else num(c), None, None, "births to ages 15-19", "ma-births-2023")
# County table.
for t in B:
    if t and t[0][:1] == ["County"]:
        for r in t[3:]:
            if len(r) >= 6 and r[0] and r[0] not in ("STATE TOTAL", "UNKNOWN") and not r[0].startswith("1."):
                name = r[0].title() + " County"
                births = num(r[2])
                add("county", name, "resident_births", "Total", "2023", births, None, births, None, None, "births", "ma-births-2023")
                for col, ind in [(3, "low_birthweight_pct"), (4, "preterm_pct")]:
                    n = num(r[col])
                    add("county", name, ind, "Total", "2023", n, births,
                        100 * n / births if (n is not None and births) else None, None, None, "% of births", "ma-births-2023")
        break

# Table 2: state by race and Hispanic ethnicity (main rows only).
RACES = {"State Total": "Total", "American Indian/ Alaska Native Non-Hispanic": "American Indian/Alaska Native non-Hispanic",
         "Asian/ Pacific Islander Non-Hispanic": "Asian/Pacific Islander non-Hispanic",
         "Black Non-Hispanic": "Black non-Hispanic", "Hispanic": "Hispanic", "White Non-Hispanic": "White non-Hispanic"}
t2 = None
for t in B:
    if len(t) > 3 and t[1] and t[1][0].startswith("Race and Hispanic Ethnicity (by birthing"):
        t2 = t
for r in t2[4:]:
    if r and r[0] in RACES:
        g = RACES[r[0]]
        pairs = [(9, 10, "low_birthweight_pct"), (11, 12, "adequate_prenatal_care_pct"),
                 (13, 14, "first_trimester_care_pct"), (15, 16, "cesarean_pct"), (17, 18, "breastfeeding_in_hospital_pct")]
        for ni, vi, ind in pairs:
            if g == "Total" and ind == "low_birthweight_pct":
                continue  # the town table already holds the state total
            v = num(r[vi]) if vi < len(r) and not r[vi].startswith("--") else None
            n = num(r[ni]) if not r[ni].startswith("--") else "<5"
            add("state", "Massachusetts", ind, g, "2023", n, None, v, None, None, "%", "ma-births-2023")
# Gestational age by race: preterm = very early + 28-33 + late.
t10 = None
for t in B:
    if t and t[0] and t[0][0].startswith("Gestational Age"):
        t10 = t
order = ["Total", "American Indian/Alaska Native non-Hispanic", "Asian/Pacific Islander non-Hispanic",
         "Black non-Hispanic", "Hispanic", "White non-Hispanic"]
tot = next(r for r in t10 if r and r[0] == "State Total")
pre_rows = [r for r in t10 if r and (r[0].startswith("Very Early") or r[0] == "(28-33)" or r[0].startswith("Late (34-36)"))]
for k, g in enumerate(order):
    births = num(tot[1 + 2 * k])
    ns = [num(r[1 + 2 * k]) for r in pre_rows]
    if any(x is None for x in ns):
        continue
    n = sum(ns)
    if g == "Total":
        continue  # the town table already holds the state total
    add("state", "Massachusetts", "preterm_pct", g, "2023", n, births, 100 * n / births, None, None, "% of births", "ma-births-2023")
# Table 1 trend (state).
t1 = B[0]
years = t1[0][2:]
def trend(label, ind):
    for i, r in enumerate(t1):
        if r and r[0].startswith(label):
            vals = t1[i + 1] if r[1] == "n" else r
            for y, v in zip(years, (t1[i + 1] if r[1] == "n" else r)[2:]):
                if num(v) is not None and y.isdigit() and int(y) >= 2013:
                    add("state", "Massachusetts", ind, "Total", y, None, None, num(v), None, None, "% of births", "ma-births-2023")
            return
trend("Low Birthweight", "low_birthweight_pct_trend")
trend("Preterm", "preterm_pct_trend")
for r in t1:
    if r and r[0].startswith("APNCU Index"):
        for y, v in zip(years, r[2:]):
            if num(v) is not None and y.isdigit() and int(y) >= 2013:
                add("state", "Massachusetts", "adequate_prenatal_care_pct_trend", "Total", y, None, None, num(v), None, None, "%", "ma-births-2023")

# Death report Table 5: infant mortality by race, 2012-2022.
D = tables(DEATH)
t5 = find(D, "Table 5. Trends in Infant")
groups = ["Total", "American Indian/Alaska Native non-Hispanic", "Asian/Pacific Islander non-Hispanic",
          "Black non-Hispanic", "Hispanic", "White non-Hispanic"]
section = None
for r in t5:
    if r and r[0].startswith("INFANT MORTALITY"):
        section = "infant_mortality_rate"
    elif r and r[0].startswith("NEONATAL"):
        section = "neonatal_mortality_rate"
    elif r and r[0].startswith("POST NEONATAL"):
        section = "postneonatal_mortality_rate"
    elif r and section and r[0].isdigit():
        for k, g in enumerate(groups):
            n, v = r[1 + 2 * k], r[2 + 2 * k]
            add("state", "Massachusetts", section, g, r[0], "<5" if v.startswith("--") else num(n), None,
                None if v.startswith("--") else num(v), None, None, "per 1,000 live births", "ma-deaths-2022")

# PRAMS 2019-2021 (weighted %, 95% CI).
P = tables(PRAMS)
PR = {"Total": "Total", "White, non-Hispanic": "White non-Hispanic", "Black, non-Hispanic": "Black non-Hispanic",
      "Hispanic": "Hispanic", "Asian, non-Hispanic": "Asian non-Hispanic"}
def prams(title, cols, period):
    t = find(P, title)
    for r in t:
        if r and r[0] in PR:
            for (ci, ind) in cols:
                cell = r[ci] if ci < len(r) else ""
                m = re.match(r"([\d.]+)\s*\(([\d.]+)-([\d.]+)\)", cell)
                if m:
                    add("state", "Massachusetts", ind, PR[r[0]], period, None, None, float(m.group(1)),
                        m.group(2), m.group(3), "% (weighted)", "ma-prams-2019-2021")
prams("Table 19.", [(2, "prams_depression_screening_prenatal_pct"), (4, "prams_depression_screening_postpartum_pct"),
                    (6, "prams_postpartum_depressive_symptoms_pct")], "2019-2021")
prams("Table 47.", [(10, "prams_breastfeeding_initiation_pct"), (12, "prams_breastfeeding_8_weeks_pct")], "2021")
prams("Table 48.", [(2, "prams_back_to_sleep_pct"), (6, "prams_no_bed_sharing_pct"), (8, "prams_no_soft_bedding_pct")], "2019-2021")
prams("Table 56.", [(10, "prams_unintended_pregnancy_pct")], "2021")

# United States (CDC NCHS provisional, 12 months ending).
for rec in json.load(open(US_BW)):
    ind = "preterm_pct" if "preterm" in rec["indicator"] else "low_birthweight_pct"
    g = {"All races and origins": "Total", "Hispanic": "Hispanic", "Non-Hispanic Black": "Black non-Hispanic",
         "Non-Hispanic White": "White non-Hispanic"}.get(rec["race_ethnicity"])
    if g:
        add("nation", "United States", ind, g, "12 months ending 2024 Q4 (provisional)", None, None,
            float(rec["rate"]), None, None, "% of births", "cdc-nchs-vsrr-births")
for rec in json.load(open(US_IM)):
    add("nation", "United States", "infant_mortality_rate", "Total", "12 months ending 2023 Q4 (provisional)",
        None, None, float(rec["rate"]), None, None, "per 1,000 live births", "cdc-nchs-vsrr-infant-mortality")

with open(OUT, "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f, lineterminator="\n")
    w.writerow(["geo_type", "geography", "indicator", "group", "period", "numerator", "denominator",
                "value", "lower", "upper", "unit", "source"])
    w.writerows(rows)
print(len(rows), "rows")
