#!/usr/bin/env python3
"""Write the Phase 17 funding announcement fixtures (PLAN-07) and register their identifiers.

Usage (from the repository root, Python 3.9+, standard library only):

    python3 -I fixtures/synthetic/generators/phase17_grants_fixtures.py

Writes two fictional announcements, so the tests need no copyrighted text:

* fixtures/synthetic/data/synthetic_nofo.docx  a federal-style notice of funding opportunity
  ("Community Prevention Capacity Grants", opportunity number SYN-HPP-2027-001) with headings,
  a key dates table, award information, a 15-page narrative limit, three attachments, and a merit
  review table of five criteria worth 100 points.
* fixtures/synthetic/data/synthetic_rfr.pdf  a Massachusetts-style request for response ("Healthy
  Communities Outreach Services", COMMBUYS bid BD-27-9999-SYN01-SYN02-900001) with five numbered
  criteria rated Highly Advantageous / Advantageous / Not Advantageous, an 8-page narrative limit,
  and a procurement calendar (a table, whose date cells the server's PDF reader does not extract,
  and a "Key dates" sentence, which it does). It is made from a DOCX by LibreOffice
  (`soffice --headless`), which must be installed to regenerate it.

Both name two fictional procurement and program contacts (names, 555 phone numbers, example.org
emails) that no tool may return. The script adds them to fixtures/synthetic/seeded_identifiers.json
by (file, kind, value). Everything is fictional; the agencies do not exist. Reruns write the same
text (the PDF's internal ids may differ).
"""
import json
import shutil
import subprocess
import tempfile
import zipfile
from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[3]
DATA = ROOT / "fixtures/synthetic/data"
SEEDED = ROOT / "fixtures/synthetic/seeded_identifiers.json"

CONTACTS = [
    ("Veronika Haldane", "413-555-0171", "veronika.haldane@example.org"),
    ("Osric Penhallow", "413-555-0172", "osric.penhallow@example.org"),
]

CT = """<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"""
RELS = """<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"""
DOC_RELS = """<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"""
STYLES = """<?xml version="1.0" encoding="UTF-8"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Liberation Serif" w:hAnsi="Liberation Serif"/><w:sz w:val="24"/><w:lang w:val="en-US"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:rPr><w:b/><w:sz w:val="36"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="80"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="30"/></w:rPr></w:style></w:styles>"""


def para(text, style=None):
    ppr = f'<w:pPr><w:pStyle w:val="{style}"/></w:pPr>' if style else ""
    return f'<w:p>{ppr}<w:r><w:t xml:space="preserve">{escape(text)}</w:t></w:r></w:p>'


def table(rows):
    out = ['<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders>'
           + "".join(f'<w:{s} w:val="single" w:sz="4" w:space="0" w:color="808080"/>' for s in ("top", "left", "bottom", "right", "insideH", "insideV"))
           + "</w:tblBorders></w:tblPr>"]
    for i, r in enumerate(rows):
        hdr = "<w:trPr><w:tblHeader/></w:trPr>" if i == 0 else ""
        cells = "".join(f'<w:tc><w:tcPr><w:tcW w:w="0" w:type="auto"/></w:tcPr>{para(c)}</w:tc>' for c in r)
        out.append(f"<w:tr>{hdr}{cells}</w:tr>")
    out.append("</w:tbl>")
    return "".join(out)


def write_docx(path, body):
    doc = ('<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>'
           + "".join(body) + "</w:body></w:document>")
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        # Fixed timestamps keep reruns byte-identical.
        for name, data in (("[Content_Types].xml", CT), ("_rels/.rels", RELS), ("word/_rels/document.xml.rels", DOC_RELS),
                           ("word/styles.xml", STYLES), ("word/document.xml", doc)):
            z.writestr(zipfile.ZipInfo(name, date_time=(2026, 11, 12, 0, 0, 0)), data)


def nofo():
    (c1, c2) = CONTACTS
    b = [
        para("Notice of Funding Opportunity: Community Prevention Capacity Grants", "Title"),
        para("Funding Opportunity Number: SYN-HPP-2027-001"),
        para("Synthetic Office of Prevention Programs (a fictional agency used for software tests)"),
        para("Overview", "Heading1"),
        para("This fictional notice offers cooperative agreements to build the prevention capacity of local public health agencies. Applicants propose a three-year project that reduces chronic disease risk in a defined community."),
        para("Eligible Applicants", "Heading1"),
        para("Eligible applicants are county health departments, municipal boards of health and health departments, regional public health districts, and federally recognized tribal health programs. Nonprofit organizations may take part only as a named partner. For-profit entities and individuals are not eligible."),
        para("Key Dates", "Heading1"),
        table([["Milestone", "Date"],
               ["Letter of intent due", "December 18, 2026"],
               ["Applicant webinar", "January 7, 2027, 2:00 PM ET"],
               ["Application due", "February 26, 2027, 11:59 PM ET"],
               ["Anticipated award notification", "May 14, 2027"],
               ["Anticipated project start", "July 1, 2027"]]),
        para("Award Information", "Heading1"),
        para("The estimated total program funding is $2,400,000 and we expect to make 8 awards. The award ceiling is $300,000 per year and the award floor is $150,000 per year. The period of performance is 36 months. Cost sharing or matching is not required."),
        para("How to Apply", "Heading1"),
        para("Applications are submitted through Grants.gov. The applicant organization must have an active SAM.gov registration and a unique entity identifier (UEI)."),
        para("Application Content and Format", "Heading1"),
        para("The Project Narrative must not exceed fifteen (15) pages. Use 12-point Times New Roman font, single spaced, with one-inch margins. The Budget Narrative must not exceed 5 pages. The abstract is limited to 300 words."),
        para("Applicants must include the following:"),
        para("- SF-424 Application for Federal Assistance"),
        para("- Budget narrative and budget form"),
        para("- Project abstract"),
        para("Attachment 1: Work Plan Template (required)"),
        para("Attachment 2: Letters of Commitment from Partners (required)"),
        para("Attachment 3: Organizational Chart (optional)"),
        para("Merit Review Criteria", "Heading1"),
        para("Reviewers score each criterion; the maximum total score is 100 points."),
        table([["Criterion", "Points"],
               ["Statement of Need", "20"],
               ["Approach and Work Plan", "30"],
               ["Evaluation and Performance Measurement", "20"],
               ["Organizational Capacity and Partnerships", "20"],
               ["Sustainability", "10"],
               ["Total", "100"]]),
        para("Agency Contacts", "Heading1"),
        para(f"Program questions: {c1[0]}, {c1[1]}, {c1[2]}. Grants management questions: {c2[0]}, {c2[1]}, {c2[2]}."),
    ]
    write_docx(DATA / "synthetic_nofo.docx", b)


def rfr(tmp):
    (c1, c2) = CONTACTS
    b = [
        para("COMMONWEALTH OF MASSACHUSETTS (FICTIONAL)", "Title"),
        para("REQUEST FOR RESPONSES (RFR)"),
        para("Healthy Communities Outreach Services"),
        para("Agency Document Number: SYN-EOES-2027-07"),
        para("COMMBUYS Bid Number: BD-27-9999-SYN01-SYN02-900001"),
        para(f"Procurement Contact Person: {c2[0]}, {c2[1]}, {c2[2]}"),
        para("1. Description or Purpose of Procurement", "Heading1"),
        para("The Executive Office of Example Services, a fictional agency, is issuing this RFR to select up to four bidders to provide community health outreach services. Bidders are advised to check COMMBUYS regularly for amendments."),
        para("2. Eligible Bidders", "Heading1"),
        para("Municipal health departments, regional health collaboratives, and nonprofit organizations may respond. Individuals are not eligible. Small business participants in the SBPP receive the preference described in the RFR."),
        para("3. Contract Term and Funding", "Heading1"),
        para("The initial term of the contract is twelve (12) months, with up to two one-year renewals. The maximum obligation for the initial term is $450,000."),
        para("4. Estimated Procurement Calendar", "Heading1"),
        table([["Event", "Date", "Time"],
               ["Bidder questions due (all questions must be submitted through COMMBUYS)", "12/4/2026", ""],
               ["Bidder RFR responses due", "1/15/2027", "5:00 PM"],
               ["Anticipated contract start", "3/1/2027", ""]]),
        para("Key dates: Bidder questions due 12/4/2026. Bidder RFR responses due 1/15/2027, 5:00 PM. Anticipated contract start 3/1/2027."),
        para("5. Evaluation Criteria", "Heading1"),
        para("Responses that meet the submission requirements will be rated Highly Advantageous, Advantageous, or Not Advantageous on each of the following criteria."),
        para("Criteria 1: Cover Letter"),
        para("Criteria 2: Experience and Qualifications Narrative"),
        para("The narrative must not exceed eight (8) double spaced pages in size 12 Times New Roman font."),
        para("Criteria 3: Fee Proposal"),
        para("Criteria 4: Resumes, References, and License"),
        para("Criteria 5: Approach to Community Outreach"),
        para("6. Submission of Responses", "Heading1"),
        para("Only electronic responses submitted through COMMBUYS will be accepted. Bidders must include the following:"),
        para("- Cover letter"),
        para("- Experience and qualifications narrative"),
        para("- Fee proposal"),
        para("7. RFR Attachments", "Heading1"),
        para("Appendix A: Required Specifications"),
        para("Appendix B: Electronic Quote Submission Instructions"),
        para("Appendix C: Sample Contract"),
        para(f"Questions about the RFR may also be sent to {c1[0]} at {c1[2]} or {c1[1]}."),
    ]
    docx = tmp / "synthetic_rfr.docx"
    write_docx(docx, b)
    soffice = shutil.which("soffice") or shutil.which("libreoffice")
    if not soffice:
        raise SystemExit("LibreOffice (soffice) is needed to regenerate synthetic_rfr.pdf")
    subprocess.run([soffice, "--headless", "--convert-to", "pdf", "--outdir", str(tmp), str(docx)],
                   check=True, capture_output=True)
    shutil.copyfile(tmp / "synthetic_rfr.pdf", DATA / "synthetic_rfr.pdf")


def main():
    nofo()
    with tempfile.TemporaryDirectory() as t:
        rfr(Path(t))
    seeded = []
    for f in ("synthetic_nofo.docx", "synthetic_rfr.pdf"):
        for name, phone, email in CONTACTS:
            seeded.append({"value": name, "kind": "name", "file": f"data/{f}", "column": "contact line"})
            seeded.append({"value": phone, "kind": "phone", "file": f"data/{f}", "column": "contact line"})
            seeded.append({"value": email, "kind": "email", "file": f"data/{f}", "column": "contact line"})
    d = json.loads(SEEDED.read_text(encoding="utf-8"))
    have = {(i["file"], i["kind"], i["value"]) for i in d["identifiers"]}
    for s in seeded:
        if (s["file"], s["kind"], s["value"]) not in have:
            d["identifiers"].append(s)
    SEEDED.write_text(json.dumps(d, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
