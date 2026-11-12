//! Golden and unit tests for the offline geocoder: the bundled four-town sample index
//! (real MassGIS address points and TIGER data), interpolation on a small hand-made index,
//! the ZIP and place crosswalk, and installed-index discovery and checksums.

use phn_data::SnapshotStore;
use phn_data::geocoder::index::{IndexSet, MatchLevel};
use phn_data::geocoder::{Crosswalk, normalize_place, parse_address};
use phn_data::projection::from_mass_mainland;

fn bundled() -> IndexSet {
    IndexSet::open(&[], &SnapshotStore::bundled())
}

fn parse(s: &str) -> phn_data::geocoder::ParsedAddress {
    parse_address(s, &|_| false)
}

/// Public buildings. Source: the MassGIS Master Address Data town export of 2026-11-09 (point
/// X, Y in EPSG:26986 metres), TIGER/Line 2025 block groups; checked by
/// `xtask/scripts/verify_geocoder_golden.py` against MassGIS's own 2020 block groups and the
/// tract outlines in `snapshots/ma_boundaries_tracts.csv`. Longitude and latitude are the
/// builder's projection of the unrounded MassGIS point.
/// (town id, address, x, y, block group, lon, lat)
const GOLDEN: &[(u16, &str, f64, f64, &str, f64, f64)] = &[
    (
        8,
        "4 Boltwood Avenue",
        116098.0,
        903275.0,
        "250158205003",
        -72.518755,
        42.375467,
    ),
    (
        8,
        "43 Amity Street",
        115922.0,
        903364.0,
        "250158205004",
        -72.520901,
        42.376251,
    ),
    (
        8,
        "155 College St",
        116699.0,
        902854.0,
        "250158206001",
        -72.511397,
        42.371741,
    ),
    (
        214,
        "210 Main Street",
        106688.0,
        896967.0,
        "250158219031",
        -72.631982,
        42.317619,
    ),
    (
        214,
        "20 West Street",
        106381.0,
        896880.0,
        "250158219031",
        -72.635688,
        42.316798,
    ),
    (
        214,
        "54 Round Hill Road",
        105983.0,
        897701.0,
        "250158219011",
        -72.640658,
        42.32414,
    ),
    (
        230,
        "351 Amherst Road",
        125128.0,
        904898.0,
        "250158202053",
        -72.409329,
        42.390995,
    ),
    (
        6,
        "5 Alford Center Road",
        41998.0,
        889031.0,
        "250039343001",
        -73.414373,
        42.23578,
    ),
];

#[test]
fn the_sample_covers_four_towns() {
    let set = bundled();
    assert_eq!(set.sample_towns(), vec![6, 8, 214, 230]);
    let info = set.info();
    assert_eq!(info.kind, "sample");
    assert_eq!(info.towns_covered, 4);
    let (points, ranges) = set.town(8).unwrap().counts();
    assert!(points > 6000 && ranges > 1000, "{points} {ranges}");
}

#[test]
fn public_buildings_are_found_at_their_massgis_points() {
    let set = bundled();
    for (town, address, x, y, bg, lon, lat) in GOLDEN {
        let l = set
            .town(*town)
            .unwrap()
            .locate(&parse(address))
            .unwrap_or_else(|| panic!("{address} not found"));
        assert_eq!(l.level, MatchLevel::ExactPoint, "{address}");
        assert_eq!(l.block_group.as_deref(), Some(*bg), "{address}");
        let (px, py) = l.xy.unwrap();
        assert!(
            (px - x).abs() <= 1.0 && (py - y).abs() <= 1.0,
            "{address}: {px} {py}"
        );
        let (la, lo) = from_mass_mainland(px, py);
        assert!(
            (la - lat).abs() < 2e-5 && (lo - lon).abs() < 2e-5,
            "{address}: {la} {lo}"
        );
        // The tract is the first eleven digits of the block group.
        assert_eq!(&bg[..5], if *town == 6 { "25003" } else { "25015" });
    }
}

#[test]
fn spellings_of_the_same_address_find_the_same_point() {
    let set = bundled();
    let t = set.town(214).unwrap();
    let base = t.locate(&parse("20 West Street")).unwrap();
    for s in [
        "20 W St",
        "20 west st.",
        "20 WEST STREET APT 4B",
        "20 West St #3",
        "20 W. Street Unit 2",
    ] {
        assert_eq!(t.locate(&parse(s)).unwrap(), base, "{s}");
    }
    // A street typed without its type, or with the wrong one, when only one street has the name.
    let amherst = set.town(8).unwrap();
    let boltwood = amherst.locate(&parse("4 Boltwood Avenue")).unwrap();
    assert_eq!(amherst.locate(&parse("4 Boltwood")).unwrap(), boltwood);
    assert_eq!(amherst.locate(&parse("4 Boltwood Road")).unwrap(), boltwood);
}

#[test]
fn numbers_between_points_and_along_ranges_are_interpolated() {
    let set = bundled();
    let amherst = set.town(8).unwrap();
    // Boltwood Avenue has points at 14 and 18 in the same block group: 16 lies between them.
    let l = amherst.locate(&parse("16 Boltwood Avenue")).unwrap();
    assert_eq!(l.level, MatchLevel::Interpolated);
    assert_eq!(l.block_group.as_deref(), Some("250158205003"));
    let (x, y) = l.xy.unwrap();
    let a = amherst
        .locate(&parse("14 Boltwood Avenue"))
        .unwrap()
        .xy
        .unwrap();
    let b = amherst
        .locate(&parse("18 Boltwood Avenue"))
        .unwrap()
        .xy
        .unwrap();
    assert!((x - (a.0 + b.0) / 2.0).abs() < 1e-6 && (y - (a.1 + b.1) / 2.0).abs() < 1e-6);
    // Windridge Street has TIGER address ranges and no MassGIS points.
    let l = amherst.locate(&parse("17 Windridge St")).unwrap();
    assert_eq!(l.level, MatchLevel::Interpolated);
    assert!(l.block_group.is_some() && l.xy.is_some());
    // A number beyond every range on a street that crosses block groups: street only, no block group.
    let l = amherst.locate(&parse("900 Boltwood Avenue")).unwrap();
    assert_eq!(l.level, MatchLevel::StreetOnly);
    assert_eq!(l.block_group, None);
    // A street that is not in the town.
    assert!(amherst.locate(&parse("5 Nonesuch Lane")).is_none());
    // No number: the street only.
    assert_eq!(
        amherst.locate(&parse("Boltwood Avenue")).unwrap().level,
        MatchLevel::StreetOnly
    );
}

#[test]
fn the_crosswalk_maps_zip_codes_and_place_names_to_towns() {
    let store = SnapshotStore::bundled();
    let towns: Vec<(u16, String)> = vec![
        (214, "Northampton".into()),
        (8, "Amherst".into()),
        (35, "Boston".into()),
        (75, "Dennis".into()),
    ];
    let cw = Crosswalk::load(&store, &towns).unwrap();
    // One town holds almost every address of 01060.
    let t = cw.zip_town("01060", 0.9).unwrap();
    assert_eq!(t.town, "Northampton");
    assert!(t.share > 0.99);
    // 01002 is mostly Amherst, partly Pelham; 01230 reaches several towns.
    let z = cw.zip_towns("01002");
    assert_eq!(z[0].town, "Amherst");
    assert!(z.iter().any(|t| t.town == "Pelham"));
    assert!(cw.zip_town("01230", 0.9).is_none());
    assert!(cw.zip_towns("01230").len() >= 4);
    // Boston has no MassGIS export; its ZIP codes come from TIGER address ranges.
    assert_eq!(cw.zip_town("02134", 0.9).unwrap().town, "Boston");
    // A ZIP code that is not in Massachusetts.
    assert!(cw.zip_towns("06103").is_empty());
    // Village and post-office names.
    assert_eq!(
        cw.town_by_name(&normalize_place("Florence")).unwrap().1,
        "Northampton"
    );
    assert_eq!(
        cw.town_by_name(&normalize_place("South Dennis")).unwrap().1,
        "Dennis"
    );
    assert_eq!(
        cw.town_by_name(&normalize_place("town of Amherst"))
            .unwrap()
            .1,
        "Amherst"
    );
    assert!(cw.town_by_name(&normalize_place("Hartford")).is_none());
    assert!(cw.is_place("FLORENCE"));
}

const TOY: &str = "#PHNGEO 1\n\
T\t900\tTestville\t015\n\
B\t250150000011 250150000012\n\
Z\t01000\n\
S\tELM AVE\t2,0,0,1000,2000\t\n\
S\tELM ST\t2,0,0,1000,2000 2,0,0,20,0 2,1,0,20,0\t1,99,oL,0,0,5000,6000,100,0 2,98,eR,1,0,5000,6000,100,0\n\
S\tOAK ST\t10,0,0,0,0 10,0,0,100,0\t\n\
S\tPINE ST\t2,0,0,0,0 6,1,0,60,0\t\n\
S\tN MAIN ST\t1,0,0,0,0 2,0,0,10,0\t\n\
S\tROOT ST\t\t1,99,oL,0,0,0,0,100,0\n";

fn toy() -> phn_data::geocoder::TownIndex {
    IndexSet::from_text(TOY)
        .unwrap()
        .town(900)
        .unwrap()
        .as_ref()
        .clone()
}

#[test]
fn interpolation_between_points_along_ranges_and_on_one_block_group_streets() {
    let t = toy();
    let at = |s: &str| t.locate(&parse(s));
    // Exact, with the point's block group.
    let l = at("4 Elm St").unwrap();
    assert_eq!(
        (l.level, l.block_group.as_deref(), l.xy),
        (
            MatchLevel::ExactPoint,
            Some("250150000011"),
            Some((1020.0, 2000.0))
        )
    );
    let l = at("6 Elm St").unwrap();
    assert_eq!(l.block_group.as_deref(), Some("250150000012"));
    // Same-parity neighbors in the same block group: linear between them.
    let l = at("14 Oak St").unwrap();
    assert_eq!(
        (l.level, l.xy),
        (MatchLevel::Interpolated, Some((40.0, 0.0)))
    );
    // Neighbors in different block groups are not used; no range: the street has two, so no block group.
    let l = at("4 Pine St").unwrap();
    assert_eq!((l.level, l.block_group), (MatchLevel::StreetOnly, None));
    // Odd number on Elm St: no odd points; the odd (left) TIGER range 1-99 gives a position 15 m
    // to the left of an eastbound segment (north: +y), a fraction (3-1)/98 along it.
    let l = at("3 Elm St").unwrap();
    assert_eq!(
        (l.level, l.block_group.as_deref()),
        (MatchLevel::Interpolated, Some("250150000011"))
    );
    let (x, y) = l.xy.unwrap();
    assert!(
        (x - (5000.0 + 100.0 * 2.0 / 98.0)).abs() < 1e-9 && (y - 6015.0).abs() < 1e-9,
        "{x} {y}"
    );
    // The even (right) range puts the point south of the segment.
    let l = at("8 Elm St").unwrap();
    assert_eq!(
        (l.level, l.block_group.as_deref()),
        (MatchLevel::Interpolated, Some("250150000012"))
    );
    assert!((l.xy.unwrap().1 - 5985.0).abs() < 1e-9);
    // A street whose every point and range is in one block group: street only, with that block group.
    let l = at("77 Oak St").unwrap();
    assert_eq!(
        (l.level, l.block_group.as_deref()),
        (MatchLevel::StreetOnly, Some("250150000011"))
    );
    // Range-only street.
    let l = at("51 Root St").unwrap();
    assert_eq!(l.level, MatchLevel::Interpolated);
    // Relaxed street names: a missing type is accepted when one street has the name; two do not.
    assert!(at("4 Oak").is_some());
    assert!(at("4 Elm").is_none(), "Elm St and Elm Ave: ambiguous");
    assert_eq!(at("4 Elm Street").unwrap().level, MatchLevel::ExactPoint);
    // A leading direction may be left out.
    assert_eq!(at("1 Main St").unwrap().level, MatchLevel::ExactPoint);
    // A number with a suffix falls back to the plain number.
    assert_eq!(at("4A Elm St").unwrap().level, MatchLevel::ExactPoint);
}

#[test]
fn an_installed_index_is_used_checked_and_falls_back_to_the_sample() {
    let dir = tempfile::tempdir().unwrap();
    let town_file = "T900.idx";
    std::fs::write(dir.path().join(town_file), TOY).unwrap();
    let sha = phn_data::snapshots::sha256_hex(TOY.as_bytes());
    let manifest = format!(
        r#"{{"format":"phn-geocoder-index","format_version":1,"kind":"full","built":"2026-11-12","towns":{{"900":{{"file":"{town_file}","sha256":"{sha}"}}}}}}"#
    );
    std::fs::write(dir.path().join("manifest.json"), manifest).unwrap();
    let store = SnapshotStore::bundled();
    let set = IndexSet::open(&[dir.path().to_path_buf()], &store);
    assert!(set.is_installed());
    assert_eq!(set.info().kind, "installed");
    assert_eq!(set.town(900).unwrap().name, "Testville");
    // Towns the installed index lacks come from the sample.
    assert_eq!(set.town(8).unwrap().name, "Amherst");
    assert!(set.town(1).is_none());

    // A modified town file fails its checksum and is not used.
    std::fs::write(dir.path().join(town_file), TOY.replace("ELM ST", "ELM XX")).unwrap();
    let set = IndexSet::open(&[dir.path().to_path_buf()], &store);
    assert!(set.town(900).is_none());
    assert!(
        set.info().warnings.iter().any(|w| w.contains("checksum")),
        "{:?}",
        set.info().warnings
    );

    // An unsupported format version is ignored with a warning.
    std::fs::write(
        dir.path().join("manifest.json"),
        r#"{"format":"phn-geocoder-index","format_version":9,"towns":{}}"#,
    )
    .unwrap();
    let set = IndexSet::open(&[dir.path().to_path_buf()], &store);
    assert!(!set.is_installed());
    assert_eq!(set.info().kind, "sample");
    assert!(!set.info().warnings.is_empty());
}

#[test]
fn malformed_index_text_is_an_error_without_values() {
    for bad in [
        "S\tX\t\t\n",
        "#PHNGEO 2\n",
        "#PHNGEO 1\nQ\tx\n",
        "#PHNGEO 1\nS\tX\tzz,0,0,0,0\t\n",
    ] {
        let e = IndexSet::from_text(bad).err().expect(bad).to_string();
        assert!(e.contains("geocoder index line"), "{e}");
    }
}
