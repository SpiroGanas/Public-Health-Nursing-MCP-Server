#!/usr/bin/env python3
"""Check the bundled sample geocoder index against independent sources and print the golden values.

Usage (Python 3.10+ with numpy; the downloads of build_geocoder_index.py must be in WORK_DIR):

    python3 -I xtask/scripts/verify_geocoder_golden.py WORK_DIR

It compares the sample index (snapshots/ma_geocoder_sample.idx; census block groups from the
Census Bureau's TIGER/Line files) with two sources the index does not use:

* MassGIS's own 2020 block groups (CENSUS2020_BLK_BG_TRCT.zip, downloaded into WORK_DIR/massgis),
  for every address point of the four sample towns, and
* the tract outlines in snapshots/ma_boundaries_tracts.csv (MassGIS 2020 tracts, simplified),
  for the golden addresses.

and prints the golden addresses (public buildings) with the coordinates in the MassGIS file, the
block group and tract, ready for crates/phn-data/tests/geocoder_golden.rs.
"""

import importlib.util
import sys
import zipfile
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("bgi", HERE / "build_geocoder_index.py")
bgi = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bgi)

GOLDEN = [  # (town id, house number, street as MassGIS writes it, label)
    (8, "4", "BOLTWOOD AVENUE", "Amherst Town Hall"),
    (8, "43", "AMITY STREET", "Jones Library, Amherst"),
    (8, "155", "COLLEGE STREET", "Amherst College Campus Police"),
    (214, "210", "MAIN STREET", "Northampton City Hall"),
    (214, "20", "WEST STREET", "Forbes Library, Northampton"),
    (214, "54", "ROUND HILL ROAD", "Clark School, Northampton"),
    (230, "351", "AMHERST ROAD", "Pelham Town Hall"),
    (6, "5", "ALFORD CENTER ROAD", "Alford Center Road (police department and children's library)"),
]


def projected_polygons(zip_path, member):
    zf = zipfile.ZipFile(zip_path)
    recs = bgi.read_dbf(zf.read(member + ".dbf"))
    shapes = bgi.read_shp(zf.read(member + ".shp"))
    p = bgi.Polygons.__new__(bgi.Polygons)
    p.keys, p.rings, p.bbox = [], [], []
    for r, (parts, pts) in zip(recs, shapes):
        bounds = list(parts) + [len(pts)]
        p.keys.append(r)
        p.rings.append([pts[bounds[i]:bounds[i + 1]] for i in range(len(parts))])
        p.bbox.append((pts[:, 0].min(), pts[:, 1].min(), pts[:, 0].max(), pts[:, 1].max()))
    return p


def decode_tracts():
    import csv
    out = {}
    with open(bgi.SNAP / "ma_boundaries_tracts.csv", newline="", encoding="utf-8") as f:
        for r in csv.DictReader(f):
            rings = []
            for poly in r["geometry"].split("|"):
                for ring in poly.split(";"):
                    nums = [int(n) for n in ring.split()]
                    if len(nums) < 6:
                        continue
                    xy, x, y = [], 0, 0
                    for i in range(0, len(nums), 2):
                        x, y = (nums[i], nums[i + 1]) if i == 0 else (x + nums[i], y + nums[i + 1])
                        xy.append((x * 10.0, y * 10.0))
                    rings.append(np.array(xy))
            out[r["id"]] = rings
    return out


def point_in(rings, x, y):
    inside = False
    for ring in rings:
        ring = np.vstack([ring, ring[:1]])  # the outlines in the snapshot are not closed
        x0, y0, x1, y1 = ring[:-1, 0], ring[:-1, 1], ring[1:, 0], ring[1:, 1]
        dy = np.where(y1 == y0, 1.0, y1 - y0)
        c = ((y0 > y) != (y1 > y)) & (x < (x1 - x0) / dy * (y - y0) + x0)
        inside ^= bool(c.sum() & 1)
    return inside


def main():
    work = Path(sys.argv[1])
    src = bgi.Sources(work)
    zpath = work / "massgis/CENSUS2020_BLK_BG_TRCT.zip"
    if not zpath.exists():
        bgi.fetch(bgi.MASSGIS + "shapefiles/census2020/CENSUS2020_BLK_BG_TRCT.zip", zpath)
    mg = projected_polygons(zpath, "CENSUS2020BLOCKGROUPS_POLY")
    mg_geoid = [r["GEOID20"] if "GEOID20" in r else r.get("GEOID") for r in mg.keys]
    from_idx = {}
    text = (bgi.SNAP / bgi.SAMPLE_FILE).read_text(encoding="utf-8")
    # points by (town, street key, number, suffix) -> (bg geoid, x, y)
    town = None
    for line in text.splitlines():
        f = line.split("\t")
        if f[0] == "T":
            town, bgs = int(f[1]), []
        elif f[0] == "B":
            bgs = f[1].split()
        elif f[0] == "S" and f[2]:
            n = x = y = 0
            for item in f[2].split(" "):
                head, b, z, dx, dy = item.split(",")
                num, _, sfx = head.partition(".")
                n += int(num)
                x += int(dx)
                y += int(dy)
                from_idx[(town, f[1], n, sfx)] = (bgs[int(b)] if b != "-" else None, x, y)
    # 1. every point of the sample towns against MassGIS block groups
    agree = total = 0
    differ = []
    keys = list(from_idx)
    px = np.array([from_idx[k][1] for k in keys], float)
    py = np.array([from_idx[k][2] for k in keys], float)
    hit = mg.lookup(px, py)
    for k, h in zip(keys, hit):
        total += 1
        theirs = mg_geoid[h] if h >= 0 else None
        if theirs == from_idx[k][0]:
            agree += 1
        else:
            differ.append((k, from_idx[k][0], theirs))
    print(f"block group of {total} sample points: {agree} agree with MassGIS's 2020 block groups "
          f"({100 * agree / total:.2f}%); {len(differ)} differ (points within a metre or two of a boundary "
          f"after rounding to 1 m, or in water)")
    # 2. The Census Bureau's own internal point of every block group (latitude and longitude),
    #    carried into metres with the builder's projection, must lie in the MassGIS block group
    #    with the same GEOID: a check of the projection the builder uses.
    import zipfile as zf2
    bgzip = src.get(bgi.BG_URL, "tiger/tl_2025_25_bg.zip")
    recs = bgi.read_dbf(bgi.zip_member(bgzip, ".dbf"), {"GEOID", "INTPTLAT", "INTPTLON"})
    lon = np.array([float(r["INTPTLON"]) for r in recs])
    lat = np.array([float(r["INTPTLAT"]) for r in recs])
    x, y = bgi.lonlat_to_mass(lon, lat)
    hit = mg.lookup(x, y)
    ok = sum(1 for r, h in zip(recs, hit) if h >= 0 and mg_geoid[h] == r["GEOID"])
    print(f"Census internal points of {len(recs)} block groups: {ok} fall in the MassGIS block group with the same GEOID")
    tracts = decode_tracts()
    print("\ngolden addresses (town id, number, street key, label, x, y, block group, tract, checks):")
    for tid, num, street, label in GOLDEN:
        key = bgi.normalize_street(street)
        found = None
        mad = src.mad_town(tid)
        for r, s in zip(*mad):
            if r and r["ADDR_NUM"] == num and r["STREETNAME"] == street and s:
                found = (s[0], s[1])
                break
        idx = from_idx.get((tid, key, int(num), ""))
        h = mg.lookup([found[0]], [found[1]])[0]
        t_ok = [tid2 for tid2, rings in tracts.items() if point_in(rings, *found)]
        print(f"  ({tid}, {num!r}, {key!r}, {label!r}): MassGIS point ({found[0]:.1f}, {found[1]:.1f}); "
              f"index {idx}; MassGIS block group {mg_geoid[h] if h >= 0 else None}; "
              f"tract outline (snapshot) {t_ok}; lon/lat {tuple(round(float(v), 6) for v in bgi.mass_to_lonlat(found[0], found[1]))}")


if __name__ == "__main__":
    main()
