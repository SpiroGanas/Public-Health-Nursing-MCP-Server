#!/usr/bin/env python3
"""Build the offline geocoder's data (Phase 15, EPI-13 geocode_and_aggregate_addresses).

Usage (from the repository root; Python 3.10+ with numpy; nothing else):

    python3 -I xtask/scripts/build_geocoder_index.py WORK_DIR --selftest
    python3 -I xtask/scripts/build_geocoder_index.py WORK_DIR --sample [--check]
    python3 -I xtask/scripts/build_geocoder_index.py WORK_DIR --crosswalk [--check]
    python3 -I xtask/scripts/build_geocoder_index.py WORK_DIR --full OUT_DIR [--towns 8,230,214]

WORK_DIR is a scratch folder for downloads (never the repository). Downloads are
kept there, so a second run is fast. Everything is read from public files:

* MassGIS Master Address Data (MAD), one point per address, per town:
  https://s3.us-east-1.amazonaws.com/download.massgis.digital.mass.gov/shapefiles/mad/town_exports/addr_pts/
  (Commonwealth of Massachusetts, MassGIS. No restrictions on use; MassGIS asks
  that the source be credited. Boston is not in the town exports.)
* U.S. Census Bureau TIGER/Line 2025: address ranges (ADDRFEAT, one file per
  county), block groups (BG) and county subdivisions (COUSUB). Public domain.

What it writes

--sample     snapshots/ma_geocoder_sample.idx: the geocoder index for a few real
             towns (Alford, Amherst, Northampton, Pelham), used by the tests and
             by the tool when no full index is installed. Prints the manifest
             fields. With --check it writes nothing and reports whether the file
             would change.
--crosswalk  snapshots/ma_geocoder_crosswalk.csv: ZIP code to town and place
             name to town tables from all MAD addresses (and, for Boston, which
             has no MAD export, from TIGER address ranges).
--full       OUT_DIR/manifest.json and OUT_DIR/T###.idx for every town (or the
             towns named with --towns): the full index a nurse installs at
             /data/geocoder/ or /cache/geocoder/. See docs/adr/0011-offline-geocoder.md.
--selftest   Checks the street-name normalizer against
             xtask/scripts/geocoder_normalizer_cases.json and the vocabulary
             against every street type in the towns it can read.

Index format (text, UTF-8, one section per town, version 1)

    #PHNGEO 1
    T<TAB>town id<TAB>town name<TAB>county FIPS (3 digits)
    B<TAB>block group GEOIDs (12 digits), space separated
    Z<TAB>ZIP codes, space separated
    S<TAB>street key<TAB>address points<TAB>address ranges        (one line per street)

An address point is  dnum[.sfx],bg,zip,dx,dy : the house number (a difference
from the previous point on the street; the first is absolute), an optional
suffix (letter or 1/2), the index of its block group in B and of its ZIP in Z
("-" when unknown), and its position in NAD83 / Massachusetts Mainland
(EPSG:26986) metres (a difference from the previous point; the first is
absolute). An address range is  from,to,<e|o|b><L|R>,bg,zip,x0,y0,dx,dy : the
TIGER range on one side of one street segment (parity even, odd, both; left or
right of the digitized direction) with the segment's end points.
"""

import argparse
import bisect
import collections
import csv
import datetime
import hashlib
import http.client
import io
import json
import re
import struct
import sys
import time
import urllib.error
import urllib.request
import zipfile
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
SNAP = ROOT / "snapshots"
VOCAB_PATH = ROOT / "crates/phn-data/data/geocoder_vocab.json"
CASES_PATH = ROOT / "xtask/scripts/geocoder_normalizer_cases.json"

MASSGIS = "https://s3.us-east-1.amazonaws.com/download.massgis.digital.mass.gov/"
MAD_URL = MASSGIS + "shapefiles/mad/town_exports/addr_pts/AddressPts_M{tid:03d}.zip"
TIGER = "https://www2.census.gov/geo/tiger/TIGER2025/"
BG_URL = TIGER + "BG/tl_2025_25_bg.zip"
COUSUB_URL = TIGER + "COUSUB/tl_2025_25_cousub.zip"
ADDRFEAT_URL = TIGER + "ADDRFEAT/tl_2025_25{county}_addrfeat.zip"

SAMPLE_TOWNS = [6, 8, 214, 230]  # Alford, Amherst, Northampton, Pelham
SAMPLE_FILE = "ma_geocoder_sample.idx"
CROSSWALK_FILE = "ma_geocoder_crosswalk.csv"
FORMAT_VERSION = 1
OFFSET_M = 15.0  # how far from the street centre line the side of a range is placed
MIN_CROSSWALK_ADDRESSES = 10
BOSTON_NEIGHBORHOODS = [
    "ALLSTON", "BACK BAY", "BEACON HILL", "BRIGHTON", "CHARLESTOWN", "DORCHESTER", "EAST BOSTON",
    "FENWAY", "HYDE PARK", "JAMAICA PLAIN", "MATTAPAN", "MISSION HILL", "NORTH END", "ROSLINDALE",
    "ROXBURY", "SOUTH BOSTON", "SOUTH END", "WEST ROXBURY",
]
MIN_CROSSWALK_SHARE = 0.01

# ------------------------------------------------------------------ vocabulary and normalizer

VOCAB = json.loads(VOCAB_PATH.read_text(encoding="utf-8"))
TYPES = VOCAB["types"]
DIRECTIONS = VOCAB["directions"]
ORDINALS = VOCAB["ordinals"]
WORDS = VOCAB["words"]
UNIT_WORDS = set(VOCAB["unit_words"])
DROP_WORDS = set(VOCAB["drop_words"])
_ORDINAL_VALUES = set(ORDINALS.values())

# Letters with accents that may appear in typed street names, and what they become.
_ACCENTS = {
    "À": "A", "Á": "A", "Â": "A", "Ã": "A", "Ä": "A", "Å": "A", "Ç": "C", "È": "E", "É": "E",
    "Ê": "E", "Ë": "E", "Ì": "I", "Í": "I", "Î": "I", "Ï": "I", "Ñ": "N", "Ò": "O", "Ó": "O",
    "Ô": "O", "Õ": "O", "Ö": "O", "Ù": "U", "Ú": "U", "Û": "U", "Ü": "U", "Ý": "Y",
}


def _is_ordinal_or_number(tok):
    return tok in _ORDINAL_VALUES or re.fullmatch(r"\d+(ST|ND|RD|TH)?", tok) is not None


def normalize_street(text):
    """Canonical street key. Must match crates/phn-data/src/geocoder/normalize.rs."""
    s = "".join(_ACCENTS.get(c, c) for c in text.upper())
    s = re.sub(r"['`’.]", "", s)
    s = re.sub(r"[^A-Z0-9#/]", " ", s)
    toks = s.split()
    for i, t in enumerate(toks):
        if i > 0 and (t in UNIT_WORDS or t.startswith("#")):
            cut = i
            if i >= 2 and t in ("FL", "FLOOR") and _is_ordinal_or_number(toks[i - 1]):
                cut = i - 1
            toks = toks[:cut]
            break
    out = []
    for t in toks:
        if t in DROP_WORDS:
            continue
        out.append(ORDINALS.get(t) or WORDS.get(t) or DIRECTIONS.get(t) or TYPES.get(t) or t)
    if len(out) >= 3 and out[0] == "STATE" and out[1] == "RT":
        out = out[1:]  # "State Route 9" and "Route 9" are the same road
    return " ".join(out)


def selftest():
    cases = json.loads(CASES_PATH.read_text(encoding="utf-8"))["cases"]
    bad = [(c["in"], normalize_street(c["in"]), c["key"]) for c in cases if normalize_street(c["in"]) != c["key"]]
    for b in bad:
        print("MISMATCH", b, file=sys.stderr)
    print(f"normalizer: {len(cases) - len(bad)} of {len(cases)} cases pass")
    return not bad


# ------------------------------------------------------------------ downloads and file formats


def sha256(b):
    return hashlib.sha256(b).hexdigest()


def fetch(url, dest, attempts=4):
    """Download url to dest unless it is already there. Returns the bytes, or None for a 404."""
    dest = Path(dest)
    if dest.exists():
        return dest.read_bytes()
    dest.parent.mkdir(parents=True, exist_ok=True)
    for i in range(attempts):
        try:
            with urllib.request.urlopen(urllib.request.Request(url), timeout=600) as r:
                data = r.read()
            dest.write_bytes(data)
            return data
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return None
            if i + 1 == attempts:
                raise
        except (http.client.IncompleteRead, ConnectionError, TimeoutError, urllib.error.URLError) as e:
            if i + 1 == attempts:
                raise
            print(f"retrying {url} after {e!r}", file=sys.stderr)
        time.sleep(5 * (i + 1))


def zip_member(data, suffix):
    zf = zipfile.ZipFile(io.BytesIO(data))
    name = next(n for n in zf.namelist() if n.lower().endswith(suffix))
    return zf.read(name)


def read_dbf(buf, want=None):
    """Records of a dBase file as dicts of stripped strings (only the fields in `want`, if given)."""
    n = struct.unpack_from("<I", buf, 4)[0]
    hl = struct.unpack_from("<H", buf, 8)[0]
    rl = struct.unpack_from("<H", buf, 10)[0]
    fields, off = [], 32
    while buf[off] != 0x0D:
        fields.append((buf[off:off + 11].split(b"\0")[0].decode(), buf[off + 16]))
        off += 32
    cols, p = [], 1
    for name, ln in fields:
        if want is None or name in want:
            cols.append((name, p, ln))
        p += ln
    out = []
    for i in range(n):
        r = buf[hl + i * rl: hl + (i + 1) * rl]
        if r[:1] == b"*":
            out.append(None)  # keep positions aligned with the shapes
            continue
        out.append({name: r[p0:p0 + ln].decode("latin-1").strip() for name, p0, ln in cols})
    return out


def read_shp(buf):
    """Shapes of a shapefile: (x, y) for points, (part start indexes, Nx2 array) for lines and polygons."""
    out, pos, end_all = [], 100, len(buf)
    while pos + 8 <= end_all:
        _, clen = struct.unpack_from(">ii", buf, pos)
        pos += 8
        st = struct.unpack_from("<i", buf, pos)[0]
        if st == 0:
            out.append(None)
        elif st == 1:
            out.append(struct.unpack_from("<dd", buf, pos + 4))
        elif st in (3, 5):
            nparts, npts = struct.unpack_from("<ii", buf, pos + 36)
            parts = struct.unpack_from("<%di" % nparts, buf, pos + 44)
            pts = np.frombuffer(buf, dtype="<f8", count=npts * 2, offset=pos + 44 + 4 * nparts).reshape(-1, 2)
            out.append((parts, pts))
        else:
            raise ValueError(f"unsupported shape type {st}")
        pos += clen * 2
    return out


# ------------------------------------------------------------------ projection (EPSG:26986)

_A = 6378137.0
_F = 1 / 298.257222101
_E = np.sqrt(2 * _F - _F * _F)


def _m(phi):
    return np.cos(phi) / np.sqrt(1 - (_E * np.sin(phi)) ** 2)


def _t(phi):
    es = _E * np.sin(phi)
    return np.tan(np.pi / 4 - phi / 2) / ((1 - es) / (1 + es)) ** (_E / 2)


_P1, _P2, _P0 = np.radians(42 + 41 / 60), np.radians(41 + 43 / 60), np.radians(41.0)
_N = (np.log(_m(_P1)) - np.log(_m(_P2))) / (np.log(_t(_P1)) - np.log(_t(_P2)))
_AF = _A * _m(_P1) / (_N * _t(_P1) ** _N)
_RHO0 = _AF * _t(_P0) ** _N


def lonlat_to_mass(lon, lat):
    """NAD83 longitude and latitude (degrees) to EPSG:26986 metres (vectorized)."""
    rho = _AF * _t(np.radians(lat)) ** _N
    theta = _N * np.radians(np.asarray(lon) + 71.5)
    return 200000.0 + rho * np.sin(theta), 750000.0 + _RHO0 - rho * np.cos(theta)


def mass_to_lonlat(x, y):
    """EPSG:26986 metres to NAD83 longitude and latitude (degrees); used to check the Rust code."""
    dx, dy = np.asarray(x) - 200000.0, _RHO0 - (np.asarray(y) - 750000.0)
    rho = np.sqrt(dx * dx + dy * dy)
    tt = (rho / _AF) ** (1 / _N)
    phi = np.pi / 2 - 2 * np.arctan(tt)
    for _ in range(8):
        es = _E * np.sin(phi)
        phi = np.pi / 2 - 2 * np.arctan(tt * ((1 - es) / (1 + es)) ** (_E / 2))
    return np.degrees(np.arctan2(dx, dy) / _N) - 71.5, np.degrees(phi)


# ------------------------------------------------------------------ point in polygon


class Polygons:
    """Polygons (several rings each, even-odd rule) for point lookups, in projected metres."""

    def __init__(self, keys, shapes_lonlat):
        self.keys = list(keys)
        self.rings = []
        self.bbox = []
        for parts, pts in shapes_lonlat:
            x, y = lonlat_to_mass(pts[:, 0], pts[:, 1])
            xy = np.column_stack([x, y])
            bounds = list(parts) + [len(xy)]
            self.rings.append([xy[bounds[i]:bounds[i + 1]] for i in range(len(parts))])
            self.bbox.append((x.min(), y.min(), x.max(), y.max()))

    def lookup(self, px, py, subset=None):
        """Index (into keys) of the polygon holding each point, or -1."""
        px, py = np.asarray(px, float), np.asarray(py, float)
        res = np.full(len(px), -1, dtype=np.int64)
        order = range(len(self.keys)) if subset is None else subset
        for pi in order:
            b = self.bbox[pi]
            cand = np.nonzero((res < 0) & (px >= b[0]) & (px <= b[2]) & (py >= b[1]) & (py <= b[3]))[0]
            if cand.size == 0:
                continue
            x, y = px[cand], py[cand]
            inside = np.zeros(cand.size, bool)
            for ring in self.rings[pi]:
                x0, y0, x1, y1 = ring[:-1, 0], ring[:-1, 1], ring[1:, 0], ring[1:, 1]
                dy = y1 - y0
                dy[dy == 0] = 1.0
                slope = (x1 - x0) / dy
                for s in range(0, cand.size, 256):
                    xs, ys = x[s:s + 256, None], y[s:s + 256, None]
                    crossing = ((y0 > ys) != (y1 > ys)) & (xs < slope * (ys - y0) + x0)
                    inside[s:s + 256] ^= (crossing.sum(axis=1) & 1).astype(bool)
            res[cand[inside]] = pi
        return res


# ------------------------------------------------------------------ source data


class Sources:
    """Downloads and parses the public sources; remembers each file's SHA-256."""

    def __init__(self, work):
        self.work = Path(work)
        self.sha = {}
        self.towns = {}  # id -> (name, county3)
        with open(SNAP / "ma_municipalities.csv", newline="", encoding="utf-8") as f:
            for r in csv.DictReader(f):
                self.towns[int(r["town_id"])] = (r["name"], r["county_fips"][-3:])
        self._bg = self._cousub = None

    def get(self, url, rel):
        data = fetch(url, self.work / rel)
        if data is not None:
            self.sha[rel] = sha256(data)
        return data

    def block_groups(self):
        """{county3: Polygons} of 2020 block groups; keys are 12-digit GEOIDs."""
        if self._bg is None:
            data = self.get(BG_URL, "tiger/tl_2025_25_bg.zip")
            recs = read_dbf(zip_member(data, ".dbf"), {"GEOID", "COUNTYFP"})
            shapes = read_shp(zip_member(data, ".shp"))
            by = collections.defaultdict(lambda: ([], []))
            for r, s in zip(recs, shapes):
                by[r["COUNTYFP"]][0].append(r["GEOID"])
                by[r["COUNTYFP"]][1].append(s)
            self._bg = {c: Polygons(k, s) for c, (k, s) in by.items()}
        return self._bg

    def cousubs(self):
        """{county3: Polygons} of county subdivisions; keys are MassGIS town ids (0 = not a town)."""
        if self._cousub is None:
            data = self.get(COUSUB_URL, "tiger/tl_2025_25_cousub.zip")
            recs = read_dbf(zip_member(data, ".dbf"), {"NAME", "COUNTYFP"})
            shapes = read_shp(zip_member(data, ".shp"))
            by_name = {}
            for tid, (name, county) in self.towns.items():
                by_name[(county, name.upper())] = tid
            by = collections.defaultdict(lambda: ([], []))
            missing = []
            for r, s in zip(recs, shapes):
                nm = r["NAME"].upper()
                tid = by_name.get((r["COUNTYFP"], nm))
                for tail in (" TOWN", " CITY"):  # TIGER writes "Amherst Town" for some, in NAME
                    if tid is None and nm.endswith(tail):
                        tid = by_name.get((r["COUNTYFP"], nm[: -len(tail)]))
                if tid is None and nm == "MANCHESTER":
                    tid = by_name.get((r["COUNTYFP"], "MANCHESTER-BY-THE-SEA"))
                if tid is None and not r["NAME"].lower().startswith("county subdivisions not defined"):
                    missing.append((r["COUNTYFP"], r["NAME"]))
                by[r["COUNTYFP"]][0].append(tid or 0)
                by[r["COUNTYFP"]][1].append(s)
            if missing:
                print("county subdivisions without a MassGIS town:", missing, file=sys.stderr)
            self._cousub = {c: Polygons(k, s) for c, (k, s) in by.items()}
        return self._cousub

    def mad_town(self, tid, shapes=True):
        """(dbf records, shapes) of one town's address points, or None (Boston has no export)."""
        data = self.get(MAD_URL.format(tid=tid), f"mad/AddressPts_M{tid:03d}.zip")
        if data is None:
            return None
        recs = read_dbf(zip_member(data, ".dbf"))
        return recs, (read_shp(zip_member(data, ".shp")) if shapes else None)

    def addrfeat(self, county):
        data = self.get(ADDRFEAT_URL.format(county=county), f"tiger/tl_2025_25{county}_addrfeat.zip")
        recs = read_dbf(
            zip_member(data, ".dbf"),
            {"FULLNAME", "LFROMHN", "LTOHN", "RFROMHN", "RTOHN", "ZIPL", "ZIPR", "PARITYL", "PARITYR"},
        )
        return recs, read_shp(zip_member(data, ".shp"))


# ------------------------------------------------------------------ towns


RANK = {"BC": 0, "BMP": 1, "BMPC": 1, "BEP": 2, "ABC": 2, "DBMP": 3, "PC": 4, "NBAC": 5, "NBSC": 5}


def _num(s):
    try:
        return int(s)
    except ValueError:
        return 0


def edge_sides(src, county):
    """Address range sides of a county: list of dicts with key, range, parity, side, ends, offset point."""
    recs, shapes = src.addrfeat(county)
    rows, x0s, y0s, x1s, y1s = [], [], [], [], []
    for r, s in zip(recs, shapes):
        if r is None or s is None or not r["FULLNAME"]:
            continue
        parts, pts = s
        rows.append(r)
        x0s.append(pts[0, 0]), y0s.append(pts[0, 1]), x1s.append(pts[-1, 0]), y1s.append(pts[-1, 1])
    x0, y0 = lonlat_to_mass(np.array(x0s), np.array(y0s))
    x1, y1 = lonlat_to_mass(np.array(x1s), np.array(y1s))
    length = np.hypot(x1 - x0, y1 - y0)
    length[length == 0] = 1.0
    ux, uy = (x1 - x0) / length, (y1 - y0) / length
    mx, my = (x0 + x1) / 2, (y0 + y1) / 2
    sides = []
    for side, f, t, z, p, sign in (
        ("L", "LFROMHN", "LTOHN", "ZIPL", "PARITYL", 1.0),
        ("R", "RFROMHN", "RTOHN", "ZIPR", "PARITYR", -1.0),
    ):
        ox, oy = mx + sign * (-uy) * OFFSET_M, my + sign * ux * OFFSET_M
        for i, r in enumerate(rows):
            lo, hi = _num(re.sub(r"\D", "", r[f])), _num(re.sub(r"\D", "", r[t]))
            if not r[f] or not r[t] or (lo == 0 and hi == 0) or r[p] not in ("E", "O", "B"):
                continue
            sides.append({
                "key": normalize_street(r["FULLNAME"]), "from": lo, "to": hi,
                "par": r[p].lower(), "side": side, "zip": r[z][:5],
                "x0": int(round(x0[i])), "y0": int(round(y0[i])), "x1": int(round(x1[i])), "y1": int(round(y1[i])),
                "ox": ox[i], "oy": oy[i],
            })
    return sides


class TownIndex:
    def __init__(self, tid, name, county):
        self.tid, self.name, self.county = tid, name, county
        self.points = {}  # (key, num, sfx) -> [rank, x, y, zip]
        self.ranges = collections.defaultdict(list)  # key -> side dicts
        self.bg_of = {}  # filled by assign

    def stats(self):
        return {"points": len(self.points), "ranges": sum(len(v) for v in self.ranges.values())}


def build_town(src, tid, keep_all_ranges, sides_by_county):
    name, county = src.towns[tid]
    ti = TownIndex(tid, name, county)
    mad = src.mad_town(tid)
    if mad is not None:
        recs, shapes = mad
        for r, s in zip(recs, shapes):
            if r is None or s is None or not r["STREETNAME"]:
                continue
            num = _num(r["NUM1"])
            if num <= 0 or r["NUM1_PFX"]:
                continue
            key = normalize_street(r["STREETNAME"])
            if not key:
                continue
            rank = RANK.get(r["POINT_TYPE"], 6)
            zipc = r["ZIPCODE"][:5]
            entries = [(num, re.sub(r"[^A-Z0-9/]", "", r["NUM1_SFX"].upper()))]
            n2 = _num(r["NUM2"])
            if n2 > 0 and not r["NUM2_PFX"]:
                entries.append((n2, re.sub(r"[^A-Z0-9/]", "", r["NUM2_SFX"].upper())))
            for n, sfx in entries:
                k = (key, n, sfx)
                cur = ti.points.get(k)
                if cur is None or rank < cur[0]:
                    ti.points[k] = [rank, s[0], s[1], zipc]
    # block groups for the points
    bgs = src.block_groups()[county]
    keys = list(ti.points)
    if keys:
        px = np.array([ti.points[k][1] for k in keys])
        py = np.array([ti.points[k][2] for k in keys])
        hit = bgs.lookup(px, py)
        ti.bg_of = {k: (bgs.keys[h] if h >= 0 else None) for k, h in zip(keys, hit)}
    # address ranges whose side lies in this town
    if county not in sides_by_county:
        sides = edge_sides(src, county)
        cs = src.cousubs()[county]
        town = cs.lookup([s["ox"] for s in sides], [s["oy"] for s in sides])
        bgi = bgs.lookup([s["ox"] for s in sides], [s["oy"] for s in sides])
        for s, t, b in zip(sides, town, bgi):
            s["town"] = cs.keys[t] if t >= 0 else 0
            s["bg"] = bgs.keys[b] if b >= 0 else None
        sides_by_county[county] = sides
    by_key_parity = collections.defaultdict(lambda: ([], [], []))  # all, even, odd numbers
    for (key, n, _), _v in ti.points.items():
        a, e, o = by_key_parity[key]
        a.append(n)
        (e if n % 2 == 0 else o).append(n)
    for lst in by_key_parity.values():
        for part in lst:
            part.sort()
    for s in sides_by_county[county]:
        if s["town"] != tid:
            continue
        if not keep_all_ranges:
            lo, hi = sorted((s["from"], s["to"]))
            nums = by_key_parity.get(s["key"])
            if nums is not None:
                arr = {"b": nums[0], "e": nums[1], "o": nums[2]}[s["par"]]
                i = bisect.bisect_left(arr, lo)
                if i < len(arr) and arr[i] <= hi:
                    continue  # address points cover this range
        ti.ranges[s["key"]].append(s)
    return ti


def render_town(ti):
    """The town's section of an index file (text)."""
    bg_list = sorted({b for b in ti.bg_of.values() if b} | {s["bg"] for v in ti.ranges.values() for s in v if s["bg"]})
    zip_list = sorted({v[3] for v in ti.points.values() if v[3]} | {s["zip"] for v in ti.ranges.values() for s in v if s["zip"]})
    bgi = {b: str(i) for i, b in enumerate(bg_list)}
    zpi = {z: str(i) for i, z in enumerate(zip_list)}
    by_street = collections.defaultdict(list)
    for (key, n, sfx), v in ti.points.items():
        by_street[key].append((n, sfx, v, ti.bg_of.get((key, n, sfx))))
    lines = [
        "#PHNGEO 1",
        f"T\t{ti.tid}\t{ti.name}\t{ti.county}",
        "B\t" + " ".join(bg_list),
        "Z\t" + " ".join(zip_list),
    ]
    for key in sorted(set(by_street) | set(ti.ranges)):
        items, pn, px, py = [], 0, 0, 0
        for n, sfx, v, bg in sorted(by_street.get(key, []), key=lambda t: (t[0], t[1])):
            x, y = int(round(v[1])), int(round(v[2]))
            items.append(f"{n - pn}{'.' + sfx if sfx else ''},{bgi.get(bg, '-')},{zpi.get(v[3], '-')},{x - px},{y - py}")
            pn, px, py = n, x, y
        rng = [
            f"{s['from']},{s['to']},{s['par']}{s['side']},{bgi.get(s['bg'], '-')},{zpi.get(s['zip'], '-')},"
            f"{s['x0']},{s['y0']},{s['x1'] - s['x0']},{s['y1'] - s['y0']}"
            for s in sorted(ti.ranges.get(key, []), key=lambda s: (s["from"], s["to"], s["side"]))
        ]
        lines.append(f"S\t{key}\t{' '.join(items)}\t{' '.join(rng)}")
    return "\n".join(lines) + "\n"


def build_sections(src, towns, keep_all_ranges):
    sides_by_county, out = {}, {}
    for n, tid in enumerate(towns, 1):
        t0 = time.time()
        ti = build_town(src, tid, keep_all_ranges, sides_by_county)
        out[tid] = (render_town(ti), ti.stats())
        print(f"[{n}/{len(towns)}] {ti.name}: {ti.stats()} in {time.time() - t0:.1f}s", file=sys.stderr)
    return out


# ------------------------------------------------------------------ crosswalk


def build_crosswalk(src):
    """ZIP to town and place name to town, from MAD addresses (TIGER address ranges where MAD has none)."""
    zips = collections.Counter()
    places = collections.Counter()
    towns_by_name = {name.upper(): tid for tid, (name, _) in src.towns.items()}
    mad_digest = []
    no_mad = []
    for tid in sorted(src.towns):
        mad = src.mad_town(tid, shapes=False)
        if mad is None:
            no_mad.append(tid)
            continue
        recs, _ = mad
        for r in recs:
            if r is None or not r["STREETNAME"]:
                continue
            z = r["ZIPCODE"][:5]
            if re.fullmatch(r"\d{5}", z) and not z.startswith("000"):
                zips[(z, tid)] += 1
            comm = r["COMMUNITY"].upper()
            if comm and "," not in comm and comm != src.towns[tid][0].upper() and comm not in towns_by_name:
                places[(comm, tid)] += 1
    basis = {}
    for tid in no_mad:  # towns without a MAD export: weight TIGER ranges by the addresses they imply
        name, county = src.towns[tid]
        sides = edge_sides(src, county)
        cs = src.cousubs()[county]
        town = cs.lookup([s["ox"] for s in sides], [s["oy"] for s in sides])
        for s, t in zip(sides, town):
            if t >= 0 and cs.keys[t] == tid and re.fullmatch(r"\d{5}", s["zip"]) and not s["zip"].startswith("000"):
                span = abs(s["to"] - s["from"])
                zips[(s["zip"], tid)] += span // 2 + 1 if s["par"] != "b" else span + 1
                basis[(s["zip"], tid)] = "tiger_ranges"
    rows = []
    for kind, counter in (("zip", zips), ("place", places)):
        total = collections.Counter()
        for (key, tid), n in counter.items():
            total[key] += n
        for (key, tid), n in sorted(counter.items()):
            share = n / total[key]
            if n >= MIN_CROSSWALK_ADDRESSES and share >= MIN_CROSSWALK_SHARE:
                rows.append([kind, key, tid, src.towns[tid][0], n, f"{share:.4f}", basis.get((key, tid), "mad_points")])
    # Boston has no MAD export, so the names of its neighborhoods (which people write instead of
    # "Boston") are listed in this script, not derived from data: no address count, share 1.
    for hood in BOSTON_NEIGHBORHOODS:
        rows.append(["place", hood, 35, src.towns[35][0], 0, "1.0000", "listed_in_builder"])
    text = io.StringIO()
    w = csv.writer(text, lineterminator="\n")
    w.writerow(["kind", "key", "town_id", "town", "addresses", "share", "basis"])
    w.writerows(rows)
    return text.getvalue(), len(rows)


# ------------------------------------------------------------------ main


def combined_sha(src, prefix):
    parts = sorted((k, v) for k, v in src.sha.items() if k.startswith(prefix))
    return sha256("\n".join(f"{k} {v}" for k, v in parts).encode()), len(parts)


def manifest_fields(src, path, text, rows, extra_sha=None):
    sources = {}
    for rel, h in sorted(src.sha.items()):
        if not rel.startswith("mad/") or extra_sha is None:
            sources[Path(rel).name] = h
    if extra_sha is not None:
        h, n = combined_sha(src, "mad/")
        sources[f"AddressPts_M###.zip (combined digest of {n} files)"] = h
    return {
        "source_sha256": sources,
        "files": [{"path": path, "sha256": sha256(text.encode()), "bytes": len(text.encode()), "rows": rows}],
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("work", help="scratch folder for downloads")
    ap.add_argument("out", nargs="?", help="output folder for --full")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--sample", action="store_true")
    ap.add_argument("--crosswalk", action="store_true")
    ap.add_argument("--full", action="store_true")
    ap.add_argument("--check", action="store_true", help="write nothing; report whether snapshots would change")
    ap.add_argument("--towns", help="comma-separated MassGIS town ids (default: all, or the sample towns)")
    a = ap.parse_args()
    ok = True
    if a.selftest:
        ok = selftest()
    src = Sources(a.work)
    if a.sample:
        text = "".join(sec for tid, (sec, _) in sorted(build_sections(src, SAMPLE_TOWNS, True).items()))
        target = SNAP / SAMPLE_FILE
        same = target.exists() and target.read_text(encoding="utf-8") == text
        rows = text.count("\nS\t")
        print(json.dumps(manifest_fields(src, SAMPLE_FILE, text, rows), indent=1))
        if a.check:
            print(f"{SAMPLE_FILE}: {'unchanged' if same else 'WOULD CHANGE'}")
            ok = ok and same
        else:
            target.write_text(text, encoding="utf-8")
    if a.crosswalk:
        text, rows = build_crosswalk(src)
        target = SNAP / CROSSWALK_FILE
        same = target.exists() and target.read_text(encoding="utf-8") == text
        print(json.dumps(manifest_fields(src, CROSSWALK_FILE, text, rows, extra_sha=True), indent=1))
        if a.check:
            print(f"{CROSSWALK_FILE}: {'unchanged' if same else 'WOULD CHANGE'}")
            ok = ok and same
        else:
            target.write_text(text, encoding="utf-8")
    if a.full:
        if not a.out:
            ap.error("--full needs OUT_DIR")
        out = Path(a.out)
        out.mkdir(parents=True, exist_ok=True)
        towns = [int(t) for t in a.towns.split(",")] if a.towns else sorted(src.towns)
        t0 = time.time()
        secs = build_sections(src, towns, False)
        entries = {}
        for tid, (text, st) in secs.items():
            fn = f"T{tid:03d}.idx"
            (out / fn).write_text(text, encoding="utf-8")
            entries[str(tid)] = {"name": src.towns[tid][0], "file": fn, "sha256": sha256(text.encode()), **st}
        manifest = {
            "format": "phn-geocoder-index", "format_version": FORMAT_VERSION, "kind": "full",
            "built": datetime.date.today().isoformat(), "build_seconds": round(time.time() - t0),
            "coordinate_system": "EPSG:26986", "block_group_vintage": "2020 Census (TIGER/Line 2025)",
            "sources": [
                {"title": "MassGIS Master Address Data, address points by town", "url": MAD_URL.format(tid=0).replace("M000", "M###"),
                 "publisher": "MassGIS (Commonwealth of Massachusetts)"},
                {"title": "TIGER/Line 2025 address ranges, block groups, county subdivisions", "url": TIGER,
                 "publisher": "U.S. Census Bureau"},
            ],
            "towns": entries,
        }
        (out / "manifest.json").write_text(json.dumps(manifest, indent=1), encoding="utf-8")
        total = sum(p.stat().st_size for p in out.iterdir())
        print(f"wrote {len(entries)} towns, {total / 1e6:.1f} MB, in {time.time() - t0:.0f} s to {out}")
    if not (a.sample or a.crosswalk or a.full or a.selftest):
        ap.error("choose --selftest, --sample, --crosswalk, or --full")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
