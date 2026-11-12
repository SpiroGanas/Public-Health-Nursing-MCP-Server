#!/usr/bin/env python3
"""Build the Phase 15 preventive-services snapshot (CHR-04 preventive_services_lookup).

Usage (from the repository root, Python 3.9+, standard library only):

    python3 -I xtask/scripts/build_prevent_snapshots.py [--check]

The USPSTF web site and API (www.uspreventiveservicestaskforce.org,
data.uspreventiveservicestaskforce.org) are not reachable from the build network, so the
recommendations are read from the final recommendation statements the Task Force published in
JAMA (and earlier journals), through NCBI's PubMed E-utilities (efetch), which return each
statement's abstract: the population, the recommendation sentences, and each sentence's grade.

The script holds a hand-made list (RECS) of the A and B recommendations to bundle: for each, the
PubMed ID of the *latest* statement on the topic, the grade expected, and a phrase that must
appear in the recommendation sentence that carries that grade. The script fetches the abstracts and
refuses to write anything if a grade does not match the text it read. Nothing about a grade or a
population comes from memory: both are copied from the abstract.

It writes snapshots/uspstf_ab_recommendations.json and updates the manifest entry
"uspstf-ab-recommendations" (checksum, rows, source checksums). With --check it writes nothing and
reports whether the files would change.

Limits, repeated in the snapshot and in the tool's caveats:

* Only statements that PubMed returns for the Corporate Author "US Preventive Services Task Force"
  from 2016 on were considered, and only where the latest statement on the topic carries an A or
  B grade. A statement published after the retrieval date, or one USPSTF has since archived or
  updated, is not reflected.
* The abstract's recommendation text can leave out details that are on USPSTF's own page.
* Statements from before 2015 (for example 2008 statements on newborn screening) are not included
  because their current USPSTF status could not be checked.
"""

import datetime
import hashlib
import json
import re
import sys
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
UA = "phn-mcp-server snapshot builder (+https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server)"
EFETCH = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi"
FILE = "uspstf_ab_recommendations.json"
SNAP_ID = "uspstf-ab-recommendations"

# (id, topic, PMID of the latest statement, grade, phrase that must be in the graded sentence)
RECS = [
    ("hypertension_screening_adults", "Hypertension in adults: screening", "33904861", "A", "screening for hypertension in adults 18 years or older"),
    ("breast_cancer_screening", "Breast cancer: screening", "38687503", "B", "biennial screening mammography"),
    ("breast_cancer_risk_reducing_medication", "Breast cancer: medication use to reduce risk", "31479144", "B", "offer to prescribe risk-reducing medications"),
    ("brca_risk_assessment", "BRCA-related cancer: risk assessment, genetic counseling, and genetic testing", "31429903", "B", "assess women with a personal or family history"),
    ("cervical_cancer_screening_21_29", "Cervical cancer: screening, ages 21 to 29", "30140884", "A", "women aged 21 to 29 years"),
    ("cervical_cancer_screening_30_65", "Cervical cancer: screening, ages 30 to 65", "30140884", "A", "women aged 30 to 65 years"),
    ("colorectal_cancer_screening_50_75", "Colorectal cancer: screening, ages 50 to 75", "34003218", "A", "adults aged 50 to 75 years"),
    ("colorectal_cancer_screening_45_49", "Colorectal cancer: screening, ages 45 to 49", "34003218", "B", "adults aged 45 to 49 years"),
    ("lung_cancer_screening", "Lung cancer: screening", "33687470", "B", "annual screening for lung cancer with LDCT"),
    ("tobacco_cessation_adults", "Tobacco smoking cessation in adults (not pregnant)", "33464343", "A", "nonpregnant adults who use tobacco"),
    ("tobacco_cessation_pregnancy", "Tobacco smoking cessation in pregnant persons", "33464343", "A", "ask all pregnant persons about tobacco use"),
    ("tobacco_prevention_youth", "Tobacco use in children and adolescents: prevention", "32343336", "B", "prevent initiation of tobacco use"),
    ("prediabetes_diabetes_screening", "Prediabetes and type 2 diabetes: screening", "34427594", "B", "adults aged 35 to 70 years who have overweight or obesity"),
    ("cvd_diet_activity_counseling", "Healthy diet and physical activity for CVD prevention in adults with CVD risk factors: behavioral counseling", "33231670", "B", "adults with CVD risk factors"),
    ("statin_primary_prevention", "Statin use for primary prevention of CVD in adults", "35997723", "B", "prescribe a statin"),
    ("obesity_behavioral_intervention_adults", "Weight loss to prevent obesity-related morbidity and mortality in adults: behavioral interventions", "30326502", "B", "body mass index of 30 or higher"),
    ("falls_prevention_exercise", "Falls prevention in community-dwelling older adults: exercise interventions", "38833246", "B", "exercise interventions to prevent falls"),
    ("osteoporosis_screening_women_65_plus", "Osteoporosis: screening in women 65 and older", "39808425", "B", "women 65 years or older"),
    ("osteoporosis_screening_postmenopausal_under_65", "Osteoporosis: screening in postmenopausal women under 65 at increased risk", "39808425", "B", "postmenopausal women younger than 65 years"),
    ("aaa_screening_men_65_75_ever_smoked", "Abdominal aortic aneurysm: screening in men 65 to 75 who have ever smoked", "31821437", "B", "1-time screening for AAA"),
    ("hepatitis_b_screening_increased_risk", "Hepatitis B virus infection in adolescents and adults: screening", "33320230", "B", "adolescents and adults at increased risk"),
    ("hepatitis_b_screening_pregnancy", "Hepatitis B virus infection in pregnant women: screening", "31334800", "A", "pregnant women at their first prenatal visit"),
    ("hepatitis_c_screening", "Hepatitis C virus infection in adolescents and adults: screening", "32119076", "B", "adults aged 18 to 79 years"),
    ("hiv_screening_15_65", "HIV infection: screening, ages 15 to 65 (and others at increased risk)", "31184701", "A", "adolescents and adults aged 15 to 65 years"),
    ("hiv_screening_pregnancy", "HIV infection: screening in pregnant persons", "31184701", "A", "all pregnant persons"),
    ("hiv_prep", "HIV: preexposure prophylaxis", "37606666", "A", "prescribe preexposure prophylaxis"),
    ("syphilis_screening_nonpregnant", "Syphilis infection in nonpregnant adolescents and adults: screening", "36166020", "A", "persons who are at increased risk"),
    ("syphilis_screening_pregnancy", "Syphilis infection in pregnancy: screening", "40358930", "A", "universal screening for syphilis infection during pregnancy"),
    ("chlamydia_screening_women", "Chlamydia: screening in sexually active women", "34519796", "B", "screening for chlamydia in all sexually active women"),
    ("gonorrhea_screening_women", "Gonorrhea: screening in sexually active women", "34519796", "B", "screening for gonorrhea in all sexually active women"),
    ("sti_behavioral_counseling", "Sexually transmitted infections: behavioral counseling", "32809008", "B", "behavioral counseling for all sexually active adolescents"),
    ("latent_tb_screening", "Latent tuberculosis infection in adults: screening", "37129649", "B", "screening for LTBI in populations at increased risk"),
    ("depression_screening_adults", "Depression in adults (including pregnant and postpartum persons): screening", "37338872", "B", "screening for depression in the adult population"),
    ("anxiety_screening_adults", "Anxiety disorders in adults: screening", "37338866", "B", "screening for anxiety disorders in adults"),
    ("depression_screening_adolescents", "Major depressive disorder in adolescents 12 to 18: screening", "36219440", "B", "adolescents aged 12 to 18 years"),
    ("anxiety_screening_children_adolescents", "Anxiety in children and adolescents 8 to 18: screening", "36219403", "B", "children and adolescents aged 8 to 18 years"),
    ("perinatal_depression_prevention", "Perinatal depression: preventive counseling interventions", "30747971", "B", "pregnant and postpartum persons who are at increased risk"),
    ("unhealthy_alcohol_screening", "Unhealthy alcohol use in adults: screening and brief counseling", "30422199", "B", "unhealthy alcohol use in primary care settings in adults 18 years or older"),
    ("unhealthy_drug_use_screening", "Unhealthy drug use in adults: screening", "32515821", "B", "asking questions about unhealthy drug use in adults 18 years or older"),
    ("intimate_partner_violence_screening", "Intimate partner violence: screening in women of reproductive age", "40553450", "B", "screen for IPV in women of reproductive age"),
    ("folic_acid_supplementation", "Folic acid supplementation to prevent neural tube defects", "37526713", "A", "daily supplement containing 0.4 to 0.8 mg"),
    ("preeclampsia_aspirin", "Preeclampsia: low-dose aspirin for prevention", "34581729", "B", "low-dose aspirin"),
    ("hypertensive_disorders_pregnancy_screening", "Hypertensive disorders of pregnancy: screening", "37721605", "B", "blood pressure measurements throughout pregnancy"),
    ("gestational_diabetes_screening", "Gestational diabetes: screening at 24 weeks or later", "34374716", "B", "24 weeks of gestation or after"),
    ("gestational_weight_gain_counseling", "Healthy weight and weight gain in pregnancy: behavioral counseling", "34032823", "B", "healthy weight gain"),
    ("breastfeeding_support", "Breastfeeding: primary care behavioral counseling and support", "40198087", "B", "support breastfeeding"),
    ("asymptomatic_bacteriuria_pregnancy", "Asymptomatic bacteriuria: screening in pregnant persons", "31550038", "B", "screening pregnant persons for asymptomatic bacteriuria"),
    ("ocular_prophylaxis_newborns", "Gonococcal ophthalmia neonatorum: ocular prophylaxis for newborns", "30694327", "A", "all newborns"),
    ("fluoride_supplementation_children", "Dental caries in children younger than 5: oral fluoride supplementation", "34874412", "B", "oral fluoride supplementation"),
    ("fluoride_varnish_children", "Dental caries in children younger than 5: fluoride varnish", "34874412", "B", "fluoride varnish"),
    ("vision_screening_children_3_5", "Vision screening in children aged 3 to 5", "28873168", "B", "children aged 3 to 5 years"),
    ("high_bmi_children_interventions", "High body mass index in children and adolescents 6 and older: behavioral interventions", "38888912", "B", "high BMI"),
    ("skin_cancer_counseling_young", "Skin cancer: behavioral counseling for ages 6 months to 24 years with fair skin", "29558558", "B", "6 months to 24 years"),
]

NOT_INCLUDED = [
    "Statements published before 2015 (for example, 2008 statements on newborn screening for hearing loss, phenylketonuria, congenital hypothyroidism, and sickle cell disease) are not included because their current USPSTF status could not be checked from here.",
    "Any statement PubMed does not return for the Corporate Author 'US Preventive Services Task Force', and anything published after the retrieval date, is not included.",
    "Grades C, D, and I are not listed. Recommendations the Task Force has archived or is updating (draft statements) are not reflected.",
]

SOURCE_URLS = [
    "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?db=pubmed (one request for all PubMed IDs in the builder script)",
    "https://www.uspreventiveservicestaskforce.org/",
]

MONTHS = {m: i for i, m in enumerate(["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"], 1)}
TAG = re.compile(r"\(\s*(?:([ABCD]) recommendation|(I) statement)\s*\)\.?")


def sha(b):
    return hashlib.sha256(b).hexdigest()


def fetch(pmids):
    params = urllib.parse.urlencode({
        "db": "pubmed", "id": ",".join(pmids), "retmode": "xml",
        "tool": "public-health-nursing-mcp-server",
    })
    req = urllib.request.Request(EFETCH + "?" + params, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def clean(s):
    return re.sub(r"\s+", " ", s).strip()


def parse(xml):
    root = ET.fromstring(xml)
    out = {}
    for art in root.findall(".//PubmedArticle"):
        pmid = art.findtext(".//MedlineCitation/PMID")
        title = clean("".join(art.find(".//ArticleTitle").itertext()))
        pd = art.find(".//Journal/JournalIssue/PubDate")
        y, m, d = pd.findtext("Year"), pd.findtext("Month"), pd.findtext("Day")
        if not (y and m and d):
            ad = art.find(".//ArticleDate")
            if ad is not None:
                y, m, d = ad.findtext("Year"), ad.findtext("Month"), ad.findtext("Day")
        if m and not m.isdigit():
            m = MONTHS[m[:3]]
        published = f"{int(y):04d}-{int(m):02d}-{int(d):02d}" if (y and m and d) else None
        doi = None
        for i in art.findall(".//PubmedData/ArticleIdList/ArticleId"):
            if i.get("IdType") == "doi":
                doi = i.text
        secs = {}
        for at in art.findall(".//Abstract/AbstractText"):
            secs[(at.get("Label") or "TEXT").upper()] = clean("".join(at.itertext()))
        pop = next((t for l, t in secs.items() if l.startswith("POPULATION")), None)
        rec = " ".join(t for l, t in secs.items() if "RECOMMENDATION" in l)
        out[pmid] = {
            "pmid": pmid, "title": title, "journal": art.findtext(".//Journal/ISOAbbreviation"),
            "published": published, "doi": doi, "population": pop, "recommendation_text": rec,
        }
    return out


def graded_sentences(text):
    """Split recommendation text at each grade tag: [(sentence text, grade letter)]."""
    out, pos = [], 0
    for m in TAG.finditer(text):
        out.append((clean(text[pos:m.end()]), m.group(1) or m.group(2)))
        pos = m.end()
    return out


def build():
    pmids = sorted({r[2] for r in RECS})
    xml = fetch(pmids)
    st = parse(xml)
    missing = [p for p in pmids if p not in st]
    if missing:
        sys.exit(f"PubMed did not return: {missing}")
    statements = {}
    recs = []
    for rid, topic, pmid, grade, phrase in RECS:
        s = st[pmid]
        if not s["published"] or not s["doi"]:
            sys.exit(f"{pmid}: no date or DOI")
        sents = graded_sentences(s["recommendation_text"])
        hits = [(t, g) for t, g in sents if phrase.lower() in t.lower()]
        if len(hits) != 1:
            sys.exit(f"{rid}: phrase {phrase!r} matched {len(hits)} sentences in {pmid}")
        text, g = hits[0]
        if g != grade:
            sys.exit(f"{rid}: expected grade {grade}, the statement text says {g}: {text}")
        statements[pmid] = {
            "pmid": pmid, "title": s["title"], "journal": s["journal"], "published": s["published"],
            "doi": s["doi"], "population": s["population"],
            "url": f"https://doi.org/{s['doi']}",
            "pubmed_url": f"https://pubmed.ncbi.nlm.nih.gov/{pmid}/",
        }
        recs.append({
            "id": rid, "topic": topic, "grade": grade, "statement_pmid": pmid,
            "recommendation": text,
        })
    ids = [r["id"] for r in recs]
    if len(ids) != len(set(ids)):
        sys.exit("duplicate recommendation ids")
    doc = {
        "format_version": 1,
        "description": "USPSTF A and B recommendations read from the Task Force's published recommendation statements (abstracts, via PubMed). Grade, population, and recommendation wording are copied from the abstracts.",
        "retrieved": datetime.date.today().isoformat() if "--date" not in sys.argv else sys.argv[sys.argv.index("--date") + 1],
        "not_included": NOT_INCLUDED,
        "statements": [statements[p] for p in sorted(statements)],
        "recommendations": recs,
    }
    return doc, xml


def main():
    check = "--check" in sys.argv
    doc, xml = build()
    path = SNAP / FILE
    manifest_path = SNAP / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    old_entry = next((e for e in manifest["snapshots"] if e["id"] == SNAP_ID), None)
    if check and old_entry:
        doc["retrieved"] = old_entry["retrieved"]
    text = json.dumps(doc, indent=1, ensure_ascii=False) + "\n"
    new = text.encode("utf-8")
    old = path.read_bytes() if path.exists() else b""
    print(f"{FILE}: sha256 {sha(new)}, bytes {len(new)}, recommendations {len(doc['recommendations'])}, statements {len(doc['statements'])}")
    if check:
        print("unchanged" if old == new else "changed")
        sys.exit(0 if old == new else 1)
    path.write_bytes(new)
    dates = sorted(s["published"] for s in doc["statements"])
    entry = {
        "id": SNAP_ID,
        "title": "USPSTF A and B recommendations (from the Task Force's published recommendation statements)",
        "publisher": "U.S. Preventive Services Task Force (statements published in JAMA and other journals; read through the National Library of Medicine's PubMed E-utilities)",
        "source_urls": SOURCE_URLS,
        "source_sha256": {"pubmed_efetch_response.xml": sha(xml)},
        "source_vintage": f"statements published {dates[0]} to {dates[-1]}",
        "retrieved": doc["retrieved"],
        "license": "USPSTF recommendations are U.S. government work; the abstracts are quoted briefly with attribution. Check each journal's terms before reproducing more.",
        "derivation": "Final recommendation statements (latest per topic, 2016 on) fetched as PubMed abstracts for the PubMed IDs listed in xtask/scripts/build_prevent_snapshots.py; for each A or B recommendation the builder copies the graded sentence and the statement's population text, and refuses to write the file if the grade in the text differs from the grade in the script. Dates are the journal publication dates. Builder: xtask/scripts/build_prevent_snapshots.py (Python 3 standard library). The USPSTF web site and API could not be reached from the build network.",
        "caveats": [
            "Dates are journal publication dates; the Task Force's own posting date can differ by days.",
            "The abstract's recommendation wording omits details on the Task Force's own pages (for example screening intervals and tests). Always check the full statement.",
        ] + NOT_INCLUDED,
        "files": [{"path": FILE, "sha256": sha(new), "bytes": len(new), "rows": len(doc["recommendations"])}],
    }
    snaps = [e for e in manifest["snapshots"] if e["id"] != SNAP_ID]
    snaps.append(entry)
    manifest["snapshots"] = snaps
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print("manifest updated")


if __name__ == "__main__":
    main()
