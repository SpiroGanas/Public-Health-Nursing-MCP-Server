## Changelog

- **EPI-13 `geocode_and_aggregate_addresses` (toolset `epi`):** locates the addresses in a file in
  `/data` offline, against MassGIS Master Address Data points and Census TIGER address ranges, and
  returns only counts by town, census tract, or block group, with small counts suppressed
  (complementary suppression, because totals are shown). It reports how many addresses were located
  at each precision (exact point, interpolated, street only, ZIP code only, town only, not located)
  and the match rate, and never returns an address, coordinate, or other value from the file. It
  finds the address, town, and ZIP columns from the headers, reads only those, and refuses a name,
  birth date, phone, ID, or free-text column in their place. The full street index is optional
  (35.5 MB for 351 towns, built by `xtask/scripts/build_geocoder_index.py --full` and installed in
  `/data/geocoder/` or `/cache/geocoder/`); a four-town sample (Alford, Amherst, Northampton, Pelham;
  417 KB, real data) and a statewide ZIP-code and village-name to town crosswalk (31 KB) are bundled,
  so without the index the tool still counts by town (town column, town written in the address, or
  ZIP code). Optional `write_matched_file` writes row number, the address as written, match level,
  town, tract, block group, and latitude and longitude to `/data/output` (identifiable; only its
  name is returned). Design and index format: ADR 0011. Seeded fictional address list
  `fixtures/synthetic/data/address_list.csv` (31 rows, generator
  `fixtures/synthetic/generators/phase15_geo_fixture.py`).

## Acceptance rows

| # | Prompt | Expected tool | Check |
|---|---|---|---|
| 1 | "pertussis_homes.csv has the home addresses of this season's cases. Count them by census tract. Do not show me any addresses." | `geocode_and_aggregate_addresses` | Counts by tract (named, with GEOID) with small counts shown as `*`; a table of how many addresses were located exactly, by interpolation, by street only, by ZIP only, and not at all; no address, name, or coordinate anywhere in the reply. |
| 2 | "My program list has a ZIP column but no town column. Which towns do the clients live in?" | `geocode_and_aggregate_addresses` | Counts by town; the caveat that ZIP codes cross town lines and which share was used (90 percent); rows whose ZIP code reaches several towns are listed as not located with that reason. |
| 3 | "Geocode caseload.csv and save the locations to a file I can map myself, but only tell me the counts." | `geocode_and_aggregate_addresses` with `write_matched_file` | `/data/output/caseload_geocoded.csv` exists with the addresses, match level, tract, and latitude and longitude; the reply names the file, says it is identifiable, and has only counts. |
| 4 | "Use the Last Name column as the address column for the home visit list." | `geocode_and_aggregate_addresses` | A tool-level error saying the column appears to contain names and that only address, town, and ZIP columns are read; no name from the file in the message. |
| 5 | "Count the addresses in springfield_clients.csv by census tract." (no full index installed) | `geocode_and_aggregate_addresses` | Counts by town, with a note that no address could be placed on a street and that the full index is needed for tracts; the index used is reported as the four-town sample. |
| 6 | After installing the full index with `build_geocoder_index.py --full`: "Count the addresses in springfield_clients.csv by census tract." | `geocode_and_aggregate_addresses` | The index is reported as installed (351 towns, build date); counts by tract; a match rate to a street position above 90 percent for ordinary street addresses; PO boxes and out-of-state addresses listed as town only or not located. |
| 7 | "What is the match rate for amherst_list.csv, and which addresses did not match?" | `geocode_and_aggregate_addresses` | The match levels and rate are given; the tool does not list the unmatched addresses (it offers `write_matched_file` for the nurse's own review). |

## Content review

The tool carries two bundled data files and one vocabulary, not clinical or regulatory content;
nothing is marked `reviewed` and there is no YAML content. What the nurse should check:

- **`snapshots/ma_geocoder_sample.idx`** (`ma-geocoder-sample`): built from the real MassGIS Master
  Address Data town exports of 2026-11-09 and TIGER/Line 2025 for Alford, Amherst, Northampton, and
  Pelham. Verified: all 17,426 points fall in the same block group in MassGIS's own 2020 block-group
  layer; eight public buildings (town halls, libraries, a school) match their MassGIS points and
  the tract outlines in `ma_boundaries_tracts.csv` (golden tests). Check that the four towns suit
  your demonstrations; nothing else needs review.
- **`snapshots/ma_geocoder_crosswalk.csv`** (`ma-geocoder-crosswalk`): ZIP code to town and place
  name to town, from MassGIS address counts. **Low confidence, not verified against a source:** the
  18 Boston neighborhood names (Allston, Dorchester, Roxbury, ...), which are listed in the builder
  from general knowledge; Boston has no MassGIS export, so Boston's ZIP codes come from TIGER address
  ranges weighted by the addresses they imply. 75 of 517 ZIP codes reach more than one town;
  check the 90 percent rule (`zip_min_share`) suits the way your agency reports ZIP-based counts.
- **`crates/phn-data/data/geocoder_vocab.json`:** the street-type table is MassGIS's own
  `USPS_TYPE` abbreviation for each street type (checked on all 3.3 million points); the extra
  variants (`STR`, `AV`, `BLV`, `TRNPK`, ...) come from the USPS Publication 28 list as I know it
  (the publication's page was not fetched): **low confidence for the variants**, which only affect
  how a misspelled type is read.
- **Licensing, unverified:** MassGIS's data disclaimer page could not be retrieved. The Master
  Address Data download page and metadata state no license or use limit. Credit MassGIS (Bureau of
  Geographic Information) and check its current terms before sharing a built index. Census TIGER/Line
  is public domain.

## Open items

- **Full index is optional and not shipped.** 35.5 MB (351 towns, 2,214,732 address points, 88,991
  address-range sides; 11.7 MB compressed), 125 seconds to build after downloading about 350 MB.
  The coordinator or nurse decides whether to publish an image variant with it at
  `/cache/geocoder/`, or a release asset; the default image grew by about 450 KB (the sample and
  the crosswalk).
- **Boston:** MassGIS publishes no town export for Boston (town 35); its addresses are placed from
  TIGER ranges only (interpolated or street only). The City of Boston's own address data (Analyze
  Boston SAM) was not tried. The statewide file geodatabase (`MassGIS_AddressPoints.zip`, 580 MB)
  has Boston but needs GDAL to read.
- **Builder needs Python 3.10 and numpy** (the other builders use the standard library only).
  `--check` rebuilds the sample or crosswalk from cached downloads and reports a difference; it
  was run on the day the files were built. Source checksums are in `snapshots/manifest.json`.
- **Not done:** reverse geocoding, intersections ("Main St & Elm St"), named places, spelling
  correction of street names, PO box ZIP codes, a ZIP-code level of output, and a progress report
  for very large files (the limit is 500,000 rows, run on the blocking thread pool).
- **Measured match (full index):** 20,328 real MassGIS addresses from 120 towns, written with
  mixed abbreviations, case, and units, were all matched as exact points to the right town, and
  99.89 percent had the same block group as MassGIS's own 2020 block-group layer (the 23 differences
  are one house number on two same-named streets in one town, where the index keeps one point).
  Real client lists will match less often; the tool reports its rate on every call.
- **Matching limits:** interpolation assumes numbers are evenly spread along a straight segment;
  a MassGIS point is a building or parcel center and can lie in another block group than the front
  door; a unit-only address (`E5 Beach Plum Lane`, house numbers with a letter prefix) is matched at
  street level; two streets in a town with the same name and different types are never merged (an
  address with a wrong type is matched only when one street has the name).
- **Normalizer parity:** the Python builder and the Rust tool implement the same rules; the shared
  vocabulary file and 48 shared test vectors keep them in step. Changing the vocabulary means
  rebuilding every installed index (its keys use it); the manifest records no vocabulary version
  yet.
- **Network lessons:** the MassGIS S3 bucket lists and serves
  `shapefiles/mad/town_exports/{addr_pts,adv_addr,basic_addr}/` (350 towns; no M035), and
  `www2.census.gov/geo/tiger/TIGER2025/{ADDRFEAT,BG,COUSUB}/` work. The Census geocoder API
  (`geocoding.geo.census.gov`) returned nothing and was not used. The MassGIS MAD page
  (`mass.gov/info-details/massgis-data-master-address-data`) works with plain curl; its data
  disclaimer page could not be found.
- **Shared files touched outside the marker sections:** `snapshots/manifest.json` (two new entries
  appended at the end; merge by id), `fixtures/synthetic/seeded_identifiers.json` (149 identifiers
  for `data/address_list.csv`; merge by key), `crates/phn-data/data/` (new folder).
