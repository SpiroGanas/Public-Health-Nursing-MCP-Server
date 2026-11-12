"""Generate the fictional address list for the Phase 15 geocoder tests (standard library only).

Writes fixtures/synthetic/data/address_list.csv and adds its identifiers to
fixtures/synthetic/seeded_identifiers.json (replacing any earlier entries for the file).

The people are fictional (unusual names, 413-555-01xx phone numbers, IDs starting GEO-). The
addresses of the rows that should match exactly are public buildings that appear in the MassGIS
Master Address Data (Amherst Town Hall, Jones Library, Northampton City Hall, Forbes Library,
Pelham Town Hall, Alford's Center Road building), repeated for different fictional "clients",
so the tests know the answer. The other rows exercise interpolation, street-only and
town-only matches, ZIP-only rows, a PO box, a village name, an out-of-state address, and
an empty row. The expected result of every row is in crates/phn-server/tests/phase15_geo_tools.rs.

Run from the repository root:  python3 fixtures/synthetic/generators/phase15_geo_fixture.py
"""
import csv
import json
import random
from pathlib import Path

random.seed(20261112)
ROOT = Path("fixtures/synthetic")
FILE = "data/address_list.csv"

FIRST = ["Aldous", "Brisa", "Cormac", "Delphine", "Evander", "Fenella", "Gideon", "Halima",
         "Ignatius", "Jovanka", "Kestrel", "Lorcan", "Maelys", "Nerys", "Osric", "Perpetua",
         "Quentin", "Rosalind", "Soren", "Tamsin", "Ulrich", "Vesna", "Wilhelmina", "Xanthe",
         "Yoriko", "Zdenek", "Amadou", "Bronagh", "Casimir", "Dilys", "Emeric", "Fionnuala"]
LAST = ["Abernathy-Cole", "Brandvold", "Castellanos-Reyes", "Dunleavy", "Eisenhardt", "Fairweather",
        "Gutierrez-Moll", "Hawthornthwaite", "Idowu", "Jankowiak", "Kellermann", "Lovelock",
        "Mwangi-Frost", "Nordstrom-Hale", "Oyelaran", "Pennywhistle", "Quinlivan", "Rutherford-Bell",
        "Sandoval-Reid", "Thistlewood", "Underhill-Ng", "Vanterpool", "Wojciechowski", "Yarborough-Lee",
        "Zielinski-Park", "Ashdown", "Bellweather", "Crabtree-Osei", "Drummond-Vega", "Ellsworth-Tran",
        "Fitzhugh", "Galloway-Imani"]

# (Home Address, City, ZIP)
ROWS = [
    # Amherst Town Hall and Jones Library: exact point, written several ways
    ("4 Boltwood Avenue", "Amherst", "01002"),
    ("4 BOLTWOOD AVE.", "Amherst", "01002"),
    ("4 Boltwood Ave, Apt 2", "Amherst", "01002"),
    ("43 Amity Street", "Amherst", "01002"),
    ("43 amity st #3", "Amherst", "01002"),
    ("43 Amity St.", "Amherst, MA", "1002"),
    # Northampton City Hall and Forbes Library: exact point
    ("210 Main Street", "Northampton", "01060"),
    ("210 Main St., Unit 3", "Northampton", "01060"),
    ("20 West Street", "Northampton", "01060"),
    ("20 W St", "Northampton", "01060"),
    ("20 West St. Apt 4B", "Northampton", "01060"),
    ("210 MAIN STREET", "", "01060"),
    # the whole address in one cell, and a village name
    ("210 Main St, Northampton, MA 01060", "", ""),
    ("4 Boltwood Ave Amherst MA 01002", "", ""),
    ("20 West Street", "Florence", "01062"),
    # interpolated between neighboring points, along a TIGER range, on a street with ranges only
    ("16 Boltwood Ave", "Amherst", "01002"),
    ("24 Boltwood Ave", "Amherst", "01002"),
    ("17 Windridge St", "Amherst", "01002"),
    # street found but not the number; street not found
    ("900 Boltwood Avenue", "Amherst", "01002"),
    ("5 Nonesuch Lane", "Amherst", "01002"),
    # Pelham Town Hall and an Alford building: exact point
    ("351 Amherst Road", "Pelham", "01002"),
    ("351 Amherst Rd", "Pelham", "01002"),
    ("5 Alford Center Road", "Alford", "01230"),
    # PO box, and a town with no street index
    ("PO Box 42", "Pelham", "01002"),
    ("100 Russell Street", "Hadley", "01035"),
    # ZIP code only (one town; a ZIP that reaches several towns), and an unusable street + ZIP
    ("", "", "01060"),
    ("", "", "01002"),
    ("", "", "01230"),
    ("12 Main Street", "", "01230"),
    # not Massachusetts; nothing at all
    ("12 Main Street", "Hartford", "06103"),
    ("", "", ""),
]


def main():
    out = ROOT / FILE
    people = []
    with open(out, "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f, lineterminator="\n")
        w.writerow(["Client ID", "Last Name", "First Name", "Phone", "Home Address", "City", "ZIP"])
        for i, (addr, city, zip_) in enumerate(ROWS):
            cid = f"GEO-{7100 + i}"
            last, first = LAST[i % len(LAST)], FIRST[i % len(FIRST)]
            phone = f"413-555-{150 + i:04d}"
            w.writerow([cid, last, first, phone, addr, city, zip_])
            people.append((cid, last, first, phone, addr))
    ids_path = ROOT / "seeded_identifiers.json"
    data = json.loads(ids_path.read_text(encoding="utf-8"))
    data["identifiers"] = [x for x in data["identifiers"] if x["file"] != FILE]
    seen = set()
    for cid, last, first, phone, addr in people:
        for value, kind, col in (
            (last, "name", "Last Name"), (first, "name", "First Name"), (phone, "phone", "Phone"),
            (addr, "street_address", "Home Address"), (cid, "other_id_number", "Client ID"),
        ):
            if value and (value, kind) not in seen:
                seen.add((value, kind))
                data["identifiers"].append({"value": value, "kind": kind, "file": FILE, "column": col})
    ids_path.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {out} ({len(ROWS)} rows) and {len(seen)} seeded identifiers")


if __name__ == "__main__":
    main()
