# ADR 0011: The offline geocoder (EPI-13 `geocode_and_aggregate_addresses`)

- Status: accepted
- Date: 2026-11-12
- Phase: 15

## Context

A nurse often holds a list of addresses (cases, clients on a program, facilities) and wants to
know where they cluster. Sending the list to an online geocoder, or letting the AI model see it,
would send identifiable data to a third party. The tool therefore has to place the addresses
inside the container and return only counts by town, census tract, or block group, with small
counts suppressed. The plan (`project_plan.md`, Phase 15) calls this the largest item of the
phase and allows it to ship as an optional image variant if it makes the image too large.

Sizes were measured before choosing a design (2026-11-12):

| Source | What it is | Size |
|---|---|---|
| MassGIS Master Address Data (MAD), town exports (`shapefiles/mad/town_exports/addr_pts/AddressPts_M###.zip`) | 3,328,172 address points (2,214,732 distinct house number and street pairs), 350 towns; **no export for Boston (town 35)** | 300 MB zipped |
| Census TIGER/Line 2025 `ADDRFEAT` address ranges, 14 counties | street segments with house-number ranges and ZIP codes on each side | 40 MB zipped |
| TIGER/Line 2025 block groups and county subdivisions | 5,116 block groups; town outlines | 9 MB and 2 MB zipped |
| MassGIS statewide address points for geocoding (`gdbs/MassGIS_AddressPoints.zip`) | file geodatabase (needs GDAL) | 580 MB |

The bundled data cannot hold this: the whole statewide point set is about 35 MB as a compact
text index (12 MB compressed), and the default image and `snapshots/` may grow by about 10 MB
at most.

## Decision

### 1. Two data tiers

- **Bundled (about 450 KB):** a sample street index for four real towns (Alford, Amherst,
  Northampton, Pelham; 417,632 bytes) built from the real MassGIS and TIGER files, and a
  ZIP-to-town and place-name-to-town crosswalk for the whole state (30,648 bytes). The sample
  serves the tests and demonstrations. The crosswalk gives the town-level fallback.
- **Optional full index:** every town (351 files, 35.5 MB), built by
  `xtask/scripts/build_geocoder_index.py --full OUT_DIR` and installed in `/data/geocoder/` or
  `/cache/geocoder/` (in that order). The tool uses an installed index for the towns it lists
  and the bundled sample for the rest. An image variant for agencies that cannot build it can
  `COPY` the 35.5 MB folder to `/cache/geocoder/` (about 12 MB compressed in the layer); the
  default image does not contain it. The folder is read only; nothing writes to it at run time. To install it, build it
(`python3 -I xtask/scripts/build_geocoder_index.py WORK_DIR --full OUT_DIR`) and either place
`OUT_DIR` in the mounted data folder as `geocoder/` or mount it with
`-v /path/to/OUT_DIR:/cache/geocoder:ro`.

Why not the whole state in the binary: it would add 35 MB (more than three times the allowance)
to every installation, including those that never geocode. Why not an online service: the
requirement is that addresses never leave the container.

### 2. Index format (version 1)

Plain UTF-8 text, one section per town (a town file holds one section, the bundled sample holds
four), so it can be read, diffed, and checked without a tool:

```
#PHNGEO 1
T<TAB>town id<TAB>town name<TAB>county FIPS (3 digits)
B<TAB>block group GEOIDs (12 digits), space separated
Z<TAB>ZIP codes, space separated
S<TAB>street key<TAB>address points<TAB>address ranges        (one line per street)
```

- **Street key:** the street name after normalization (section 3), for example `N PLEASANT ST`.
- **Address point** `dnum[.sfx],bg,zip,dx,dy`: the house number as a difference from the
  previous point on the street (the first is absolute), an optional suffix (a letter or `1/2`),
  the position of its block group in `B` and of its ZIP in `Z` (`-` when unknown), and its
  position in NAD83 / Massachusetts Mainland (EPSG:26986) metres, rounded to 1 m, as a
  difference from the previous point (the first is absolute). Points are sorted by number.
- **Address range** `from,to,<e|o|b><L|R>,bg,zip,x0,y0,dx,dy`: a TIGER range on one side of one
  segment (parity even, odd, or both; left or right of the digitized direction), with the
  segment's end points. Only segments whose end points are used for interpolation are kept; the
  curve between them is not.
- An installed index also has `manifest.json` (`format` = `phn-geocoder-index`,
  `format_version` = 1, `kind`, `built`, the sources, and for every town its file name, SHA-256,
  and counts). The reader checks each town file against its checksum before using it and falls
  back to the bundled sample, with a caveat, when it does not match. A different format version
  is ignored with a caveat; rebuild it with this release's script.

Measured full index (MassGIS export last modified 2026-11-09, TIGER/Line 2025): 351 towns,
2,214,732 address points, 88,991 retained address-range sides, **35.5 MB** (11.7 MB as `tar.gz`),
built in **125 seconds** on a shared two-core container after the 350 MB of source files were
downloaded. The bundled sample is 417,632 bytes (951 streets).

### 3. Building the index

`xtask/scripts/build_geocoder_index.py` (Python 3.10+ with numpy; standard library otherwise;
`--sample`, `--crosswalk`, `--full`, `--check`, `--selftest`). For each town:

1. One point per (street key, house number, suffix) from the MAD point export; where several
   points share them (stacked units, parcel and building points) the best-ranked one is kept
   (building centroid before parcel centroid). Points with a number prefix (`E5`) or no number
   are skipped.
2. Each point is placed in a **2020 census block group** by point-in-polygon against the TIGER
   block groups (projected to EPSG:26986 with the LCC formulas of Snyder, USGS PP 1395, as in
   `phn_data::projection`). This is done once at build time, not at run time.
3. Every TIGER address range side is placed 15 m from the street's center line and assigned a
   town (county subdivision polygons) and a block group by the same method. A range is kept
   when no address point of the street lies inside it (so the index stays small and the ranges
   serve where points do not: Boston, new streets, numbers MassGIS does not list). The
   `--sample` build keeps every range.
4. Block groups nest in tracts, so the tract is the first 11 digits of the GEOID.

**Boston has no MassGIS export**; its streets come only from TIGER ranges (every Boston answer
is "interpolated" or "street only"). Verified against independent sources
(`xtask/scripts/verify_geocoder_golden.py`): all 17,426 sample points fall in the same block
group in MassGIS's own 2020 block-group layer; the golden addresses' tracts match the tract
outlines in `snapshots/ma_boundaries_tracts.csv`; and 5,106 of the 5,116 Census internal points
of the block groups fall in the MassGIS block group with the same GEOID (checks the projection).

End-to-end check with the full index installed: 20,328 real MassGIS addresses drawn at random
from 120 towns, written with full or abbreviated street types, in upper, lower, or title case, 8
percent with a unit (`Apt 2`, `#3`, `Unit B`, `2nd Floor`), went through the tool at tract level. All were
matched as exact points in the right town (100 percent), and their block group agreed with
MassGIS's own 2020 block-group layer for 20,296 of the 20,319 that layer placed (99.89 percent).
The 23 differences are the same house number on two streets with the same name in one town (the
villages of a town share street names), where the index keeps one point. The run took 1.5 seconds
in a debug build. Real lists are messier than this test, so the tool reports its match rate on
every call rather than promising one.

### 4. Street-name normalization and address parsing

`phn_data::geocoder::normalize` (Rust) and the builder (Python) apply the same steps, with the
vocabulary in one file, `crates/phn-data/data/geocoder_vocab.json`, and both are tested against
`xtask/scripts/geocoder_normalizer_cases.json` (48 cases) so the keys in an index agree with the
keys the tool computes. Steps: upper case; accents removed; apostrophes and periods dropped;
other punctuation to spaces; the unit part cut (`APT`, `UNIT`, `STE`, `FLOOR`, `#`, ...);
`REAR` dropped; ordinal words to numbers (`FIRST` to `1ST`); `SAINT`, `MOUNT`, `FORT` to
`ST`, `MT`, `FT`; `ROUTE`/`RTE` to `RT` and "State Route 9" to `RT 9`; directions (`NORTH` to
`N`) and street types to the USPS abbreviation (`STREET`, `STR` to `ST`; `AVENUE`, `AV` to `AVE`).
The street-type table is the `USPS_TYPE` abbreviation that MassGIS publishes with each point
(checked on all 3.3 million points) plus the common variants of USPS Publication 28, Appendix C1.
Matching is case-insensitive.

`parse_address` splits a cell such as `12-14 Elm St, Apt 3, Amherst, MA 01002` into house number
(the first of a range), suffix (`12A`, `12 1/2`), street key, city, and ZIP; recognizes
`PO Box`/`P.O. Box`/`Box 12` (no street position is claimed); finds a city after the street
without a comma only when a ZIP or state follows it (so "12 Amherst Road" keeps its street);
and puts a leading "Apt 3," aside.

### 5. How a row is placed

1. **Town:** the town column, else a town written in the address, else (when the name is not a
   town) the ZIP code. Town names and the village and post-office names MassGIS lists for a town
   (Florence, Hyannis, Indian Orchard) are accepted; Boston's neighborhood names are listed in the
   builder. Text that is not a Massachusetts place falls through to the ZIP code.
2. **Street lookup in that town** (when an index covers it): exact key; else the key without the
   street type or a leading direction, accepted **only when one street has that name** (a typo
   such as `Elm Road` for `Elm St` is accepted; `Elm` with both `Elm St` and `Elm Ave` is not).
3. **Position on the street:** exact point (house number and suffix; the plain number when the
   suffix is not found), else between the nearest points of the same parity in the same block group,
   else along the narrowest TIGER range that holds the number, else **street only** (a block
   group is reported only when every point and range of the street lies in one).
4. **ZIP code route** (no town): the towns that hold 2 percent or more of the ZIP code are
   searched, only when every one has an index, and the address is accepted when exactly one of
   them has it as an exact point. Otherwise a ZIP code in which one town holds at least 90 percent
   of the addresses (`zip_min_share`) is assigned to that town (**ZIP only**, an approximation);
   other ZIP codes are not assigned.

Levels reported: exact point, interpolated, street only, ZIP only, town only (a town but no
usable street), and not located (with the reason: ZIP code spans towns, not Massachusetts,
no usable address).

### 6. Privacy

- The response contains counts only: match levels, and counts by town, tract, or block group
  (named from the bundled census geography). It never contains an address, a coordinate, a
  column value, a row, or a row number.
- All counts go through `phn_epi::suppression` with complementary suppression, because totals
  are shown; areas are listed by name, not by count, so the order does not reveal a hidden
  value. The match rate is shown only when both its numerator and its complement are at least
  the threshold.
- The tool reads three columns: address, town, and ZIP (found from the headers or named). It
  uses the PHI detector's findings on the file: a column the detector takes for a name, date of
  birth, phone number, SSN, ID number, or free text is refused if the nurse (or the AI) names it
  as one of the three, and the message names the column and the kind, not a value.
  `localdata::LocalTable::column` is not used because it refuses street addresses, which is the
  one identifier this tool exists to read.
- `write_matched_file` (default false) writes row number, address as written, match level,
  town, tract, block group, and latitude and longitude to `/data/output/<file>_geocoded.csv`;
  the privacy block lists the file as identifiable and the response gives only its name.
- The leak test (`crates/phn-server/tests/mcp_integration.rs`) feeds the client list
  (`line_list.csv`), a file of contacts (`shelter_registrations.csv`), and the seeded fictional
  address list (`address_list.csv`, written by `fixtures/synthetic/generators/phase15_geo_fixture.py`)
  to the tool.

## Consequences

- The default image grows by about 450 KB. The full index is optional and its absence is never
  an error: the tool says which index it used and counts by town when it has no street data.
- Counts by tract need the installed index (or addresses in the four sample towns).
- The index must be rebuilt when MassGIS updates the address points (weekly) or the vocabulary
  changes. `build_geocoder_index.py --sample --check` and `--crosswalk --check` report whether
  the bundled files would change.
- Interpolation assumes numbers are evenly spread on a straight line between a segment's end
  points, and a point is usually a building or parcel center, so a point can fall in a different
  block group than the front door. Results are counts of rows, not people.
- **Licensing.** MassGIS publishes the Master Address Data on mass.gov for public use. Its
  download page and the data's metadata state no license or use limitation, but MassGIS's data
  disclaimer page could not be retrieved, so its terms are unverified; credit MassGIS (Bureau of
  Geographic Information, Commonwealth of Massachusetts) and check its current terms before
  redistributing a built index. MassGIS notes that addresses from utilities in the MAD were
  anonymized and that it contains voter registration list and assessor addresses. The Census
  Bureau's TIGER/Line files are in the public domain. The bundled files and every response cite
  both, with the source file dates.
- Out of scope: reverse geocoding, PO box ZIP codes, intersections, named places, and fuzzy
  spelling correction of street names.
