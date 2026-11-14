//! MCP integration tests for the Phase 15 offline geocoder (EPI-13
//! `geocode_and_aggregate_addresses`): offline calls match the output schema, every
//! row of the fictional address list is located as designed (checked row by row through
//! the optional output file), only suppressed counts are returned, other identifier
//! columns are refused and never echoed, an installed index is used and checked, and
//! nothing from the file ever appears in a response.

mod common;

use common::{everything, leaks, start, summary};
use serde_json::{Value, json};

const FILE: &str = "data/address_list.csv";

async fn call_ok(h: &common::Harness, args: Value) -> Value {
    let tools = h.client.list_all_tools().await.unwrap();
    let r = h
        .call("geocode_and_aggregate_addresses", args.clone())
        .await;
    assert_eq!(r.is_error, Some(false), "{args}: {}", everything(&r));
    assert!(!summary(&r).trim_start().starts_with('{'));
    let structured = r.structured_content.clone().expect("structured content");
    let tool = tools
        .iter()
        .find(|t| t.name == "geocode_and_aggregate_addresses")
        .unwrap();
    let schema = Value::Object((**tool.output_schema.as_ref().unwrap()).clone());
    let v = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = v
        .iter_errors(&structured)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .take(5)
        .collect();
    assert!(
        errors.is_empty(),
        "output does not match its schema: {errors:#?}"
    );
    // Nothing from the file in the response.
    let found = leaks(&everything(&r), FILE, None);
    assert!(found.is_empty(), "response contains file values: {found:?}");
    structured
}

async fn call_err(h: &common::Harness, args: Value) -> String {
    let r = h
        .call("geocode_and_aggregate_addresses", args.clone())
        .await;
    assert_eq!(
        r.is_error,
        Some(true),
        "{args} should fail: {}",
        summary(&r)
    );
    everything(&r)
}

fn caveats(s: &Value) -> String {
    s["caveats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The rows of a CSV written to /data/output as (header -> value) maps.
fn read_csv(path: &std::path::Path) -> Vec<std::collections::BTreeMap<String, String>> {
    let t = phn_local::table::read_table(path, None).unwrap();
    t.rows
        .iter()
        .map(|r| t.headers.iter().cloned().zip(r.iter().cloned()).collect())
        .collect()
}

fn area<'a>(s: &'a Value, name_part: &str) -> &'a Value {
    s["data"]["areas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["area"].as_str().unwrap().contains(name_part) || a["geoid"] == name_part)
        .unwrap_or_else(|| panic!("no area containing {name_part}: {}", s["data"]["areas"]))
}

fn level<'a>(s: &'a Value, which: &str) -> &'a Value {
    s["data"]["match_levels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["level"] == which)
        .unwrap()
}

/// Expected result of each row of `address_list.csv` (see the generator for the rows):
/// (match level, town, block group or tract prefix).
const EXPECTED: &[(&str, &str, &str)] = &[
    ("exact_point", "Amherst", "250158205003"),
    ("exact_point", "Amherst", "250158205003"),
    ("exact_point", "Amherst", "250158205003"),
    ("exact_point", "Amherst", "250158205004"),
    ("exact_point", "Amherst", "250158205004"),
    ("exact_point", "Amherst", "250158205004"), // "Amherst, MA" and the ZIP 1002 that lost its zero
    ("exact_point", "Northampton", "250158219031"),
    ("exact_point", "Northampton", "250158219031"),
    ("exact_point", "Northampton", "250158219031"),
    ("exact_point", "Northampton", "250158219031"),
    ("exact_point", "Northampton", "250158219031"),
    ("exact_point", "Northampton", "250158219031"), // no city: found through the ZIP code
    ("exact_point", "Northampton", "250158219031"), // whole address in one cell
    ("exact_point", "Amherst", "250158205003"),     // city after the street, no comma
    ("exact_point", "Northampton", "250158219031"), // the village name Florence
    ("interpolated", "Amherst", "250158205003"),    // between points 14 and 18
    ("interpolated", "Amherst", "?"),               // along a TIGER range
    ("interpolated", "Amherst", "?"),               // street with ranges only
    ("street_only", "Amherst", ""),                 // number beyond the street's ranges
    ("town_only", "Amherst", ""),                   // street not found
    ("exact_point", "Pelham", "250158202053"),
    ("exact_point", "Pelham", "250158202053"),
    ("exact_point", "Alford", "250039343001"),
    ("town_only", "Pelham", ""), // PO box
    ("town_only", "Hadley", ""), // no street index for Hadley in the sample
    ("zip_only", "Northampton", ""),
    ("zip_only", "Amherst", ""),
    ("unmatched", "", ""), // ZIP 01230 reaches several towns
    ("unmatched", "", ""),
    ("unmatched", "", ""), // Hartford
    ("unmatched", "", ""), // nothing
];

#[tokio::test]
async fn every_row_of_the_address_list_is_located_as_designed() {
    let h = start(|c| {
        c.offline = true;
        c.env_suppression_threshold = Some(2);
    })
    .await;
    let s = call_ok(
        &h,
        json!({"path": "address_list.csv", "write_matched_file": true}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["rows"], 31);
    assert_eq!(d["columns_used"]["address"], "Home Address");
    assert_eq!(d["columns_used"]["town"], "City");
    assert_eq!(d["columns_used"]["zip"], "ZIP");
    assert_eq!(d["index"]["kind"], "sample");
    assert_eq!(d["level"], "tract");
    let out = d["output_file"].as_str().unwrap();
    assert!(
        out.starts_with("output/") && out.ends_with("address_list_geocoded.csv"),
        "{out}"
    );
    assert_eq!(s["privacy"]["files_written"][0], out);
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert_eq!(s["privacy"]["suppression_applied"], true);
    assert!(s["privacy"]["notes"].to_string().contains("identifiable"));

    let rows = read_csv(&h.data().join(out));
    assert_eq!(rows.len(), EXPECTED.len());
    for (i, (r, (lvl, town, bg))) in rows.iter().zip(EXPECTED).enumerate() {
        assert_eq!(r["row"], (i + 1).to_string());
        assert_eq!(r["match_level"], *lvl, "row {}: {r:?}", i + 1);
        assert_eq!(r["town"], *town, "row {}: {r:?}", i + 1);
        match *bg {
            "" => assert_eq!(r["block_group_geoid"], "", "row {}", i + 1),
            "?" => {
                assert_eq!(r["block_group_geoid"].len(), 12, "row {}: {r:?}", i + 1);
                assert!(r["block_group_geoid"].starts_with("25015"));
            }
            bg => {
                assert_eq!(r["block_group_geoid"], bg, "row {}", i + 1);
                assert_eq!(r["tract_geoid"], bg[..11], "row {}", i + 1);
            }
        }
        // Coordinates exactly for the rows placed on a street position.
        let positioned = matches!(*lvl, "exact_point" | "interpolated");
        assert_eq!(!r["latitude"].is_empty(), positioned, "row {}", i + 1);
        if positioned {
            let (lat, lon): (f64, f64) = (
                r["latitude"].parse().unwrap(),
                r["longitude"].parse().unwrap(),
            );
            assert!(
                (42.2..42.5).contains(&lat) && (-73.5..-72.3).contains(&lon),
                "row {}: {r:?}",
                i + 1
            );
        }
    }
    // The first row is Amherst Town Hall (MassGIS point 116098, 903275, rounded to a metre).
    let (lat, lon): (f64, f64) = (
        rows[0]["latitude"].parse().unwrap(),
        rows[0]["longitude"].parse().unwrap(),
    );
    assert!(
        (lat - 42.375467).abs() < 1e-5 && (lon + 72.518755).abs() < 1e-5,
        "{lat} {lon}"
    );

    // Match levels: counts from the expected table (threshold 2). A count of 1 is hidden, and a
    // second cell (the smallest visible one) is hidden with it so it cannot be found by subtraction.
    let count = |l: &str| EXPECTED.iter().filter(|e| e.0 == l).count() as u64;
    let mut hidden = Vec::new();
    for l in [
        "exact_point",
        "interpolated",
        "street_only",
        "zip_only",
        "town_only",
        "unmatched",
    ] {
        let n = count(l);
        let item = level(&s, l);
        if item["suppressed"] == true {
            assert!(item["count"].is_null() && item["percent"].is_null(), "{l}");
            hidden.push(l);
        } else {
            assert_eq!(item["count"], n, "{l}");
        }
        if n < 2 {
            assert_eq!(
                item["suppressed"], true,
                "{l}: a count of {n} must be hidden"
            );
        }
    }
    assert!(
        hidden.contains(&"street_only") && hidden.len() == 2,
        "{hidden:?}"
    );
    assert_eq!(d["positioned_percent"], 68); // 21 of 31 rows

    // Counts by tract: tallied from the rows of the output file, and the designed exact ones.
    let mut tally = std::collections::BTreeMap::<String, u64>::new();
    for r in &rows {
        if !r["tract_geoid"].is_empty() {
            *tally.entry(r["tract_geoid"].clone()).or_default() += 1;
        }
    }
    for (tract, n) in &tally {
        let a = area(&s, tract);
        if *n >= 2 {
            assert_eq!(a["count"], *n, "{tract}");
        } else {
            assert_eq!(a["suppressed"], true, "{tract}");
        }
    }
    assert!(tally["25015820500"] >= 8 && tally["25015821903"] == 8 && tally["25015820205"] == 2);
    assert!(
        area(&s, "25015820500")["area"]
            .as_str()
            .unwrap()
            .contains("Census Tract 8205")
    );
    // A tract with one address is suppressed at threshold 2; its count is not in the result.
    assert_eq!(area(&s, "25003934300")["suppressed"], true);
    assert!(area(&s, "25003934300")["count"].is_null());
    assert!(!s["summary"].as_str().unwrap().contains("Boltwood"));
    let c = caveats(&s);
    assert!(c.contains("Boston has no MassGIS address-point download"));
    assert!(c.contains("ZIP-only rows are counted in the town"));
    assert!(c.contains("four-town sample"));

    // Why rows were not located.
    let reasons = d["not_located_reasons"].as_array().unwrap();
    assert!(reasons.iter().any(|r| {
        r["area"]
            .as_str()
            .unwrap()
            .contains("reaches several towns")
            && r["count"] == 2
    }));
    assert!(reasons.iter().any(|r| {
        r["area"]
            .as_str()
            .unwrap()
            .contains("not recognized as Massachusetts")
    }));
}

#[tokio::test]
async fn counts_by_town_and_block_group_and_small_cells() {
    let h = start(|c| c.offline = true).await;
    // Default threshold 5: only counts of 5 or more are shown, and the match-level table too.
    let s = call_ok(&h, json!({"path": "address_list.csv", "level": "town"})).await;
    let d = &s["data"];
    assert_eq!(d["level"], "town");
    assert_eq!(d["suppression_threshold"], 5);
    // 18 exact points are shown, every smaller level is hidden.
    assert_eq!(level(&s, "exact_point")["count"], 18);
    for l in [
        "interpolated",
        "street_only",
        "zip_only",
        "town_only",
        "unmatched",
    ] {
        assert_eq!(level(&s, l)["suppressed"], true, "{l}");
    }
    // Rows counted in each town (the ZIP-only rows included): Amherst 13, Northampton 9; Pelham
    // (3), Alford and Hadley (1 each), and the 4 rows not located are below the threshold.
    assert_eq!(area(&s, "Amherst")["count"], 13);
    assert_eq!(area(&s, "Northampton")["count"], 9);
    for small in ["Pelham", "Alford", "Hadley", "Not located"] {
        assert_eq!(area(&s, small)["suppressed"], true, "{small}");
    }
    assert_eq!(area(&s, "Total")["count"], 31);
    // No hidden cell stands alone: a single hidden cell could be found by subtraction from the total.
    let hidden = d["areas"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["suppressed"] == true)
        .count();
    assert_ne!(hidden, 1);
    assert!(
        s["data"]["footnote"]
            .as_str()
            .unwrap()
            .starts_with("* Counts under 5")
    );
    // Block groups.
    let s = call_ok(
        &h,
        json!({"path": "address_list.csv", "level": "block_group"}),
    )
    .await;
    assert_eq!(s["data"]["level"], "block_group");
    assert!(
        area(&s, "250158219031")["area"]
            .as_str()
            .unwrap()
            .contains("Block Group 1")
    );
}

#[tokio::test]
async fn without_street_addresses_counts_by_town_only() {
    let h = start(|c| c.offline = true).await;
    // A file with only a town column: counts by town, and the tool says why it is not finer.
    let s = call_ok(&h, json!({"path": "counts_by_town.csv"})).await;
    assert_eq!(s["data"]["level"], "town");
    assert!(
        s["data"]["level_note"]
            .as_str()
            .unwrap()
            .contains("counts are by town")
    );
    assert!(s["data"]["columns_used"]["address"].is_null());
    assert_eq!(s["data"]["columns_used"]["town"], "Town");
    // The same file with the sample's town named for every row and a made-up tract request.
    let s = call_ok(&h, json!({"path": "counts_by_town.csv", "level": "tract"})).await;
    assert_eq!(s["data"]["level"], "town");
    assert!(
        s["data"]["level_note"]
            .as_str()
            .unwrap()
            .contains("census tract")
    );
}

#[tokio::test]
async fn other_identifier_columns_are_refused_and_never_echoed() {
    let h = start(|c| c.offline = true).await;
    let ll = "data/line_list.csv";
    for (args, what) in [
        (
            json!({"path": "line_list.csv", "address_column": "Last Name"}),
            "names",
        ),
        (
            json!({"path": "line_list.csv", "address_column": "DOB"}),
            "dates of birth",
        ),
        (
            json!({"path": "line_list.csv", "address_column": "SSN"}),
            "Social Security",
        ),
        (
            json!({"path": "line_list.csv", "address_column": "Phone"}),
            "phone",
        ),
        (
            json!({"path": "line_list.csv", "town_column": "Email"}),
            "email",
        ),
        (
            json!({"path": "line_list.csv", "zip_column": "MRN"}),
            "medical record",
        ),
        (
            json!({"path": "line_list.csv", "address_column": "Notes"}),
            "free text",
        ),
    ] {
        let e = call_err(&h, args.clone()).await;
        assert!(e.contains(what), "{args}: {e}");
        assert!(leaks(&e, ll, None).is_empty(), "{args}: {e}");
        assert!(e.contains("reads only the address, town, and ZIP columns"));
    }
    // An unknown column lists the headers (not values).
    let e = call_err(
        &h,
        json!({"path": "line_list.csv", "address_column": "Home"}),
    )
    .await;
    assert!(
        e.contains("no column named 'Home'") && e.contains("Street Address"),
        "{e}"
    );
    assert!(leaks(&e, ll, None).is_empty());
    // A file without an address, town, or ZIP column.
    let e = call_err(&h, json!({"path": "phq9_screens.csv"})).await;
    assert!(e.contains("no address, town, or ZIP column"), "{e}");
    // The client list with its own address columns is counted by town; no value is returned.
    let s = call_ok(&h, json!({"path": "line_list.csv"})).await;
    assert_eq!(s["data"]["columns_used"]["address"], "Street Address");
    assert!(
        s["privacy"]["identifiers_detected"]
            .to_string()
            .contains("names")
    );
    let found = leaks(
        &everything(
            &h.call(
                "geocode_and_aggregate_addresses",
                json!({"path": "line_list.csv"}),
            )
            .await,
        ),
        ll,
        None,
    );
    assert!(found.is_empty(), "{found:?}");
    // Other bad arguments.
    let e = call_err(
        &h,
        json!({"path": "address_list.csv", "zip_min_share": 0.2}),
    )
    .await;
    assert!(e.contains("zip_min_share"));
    let e = call_err(&h, json!({"path": "address_list.csv", "town": "Amhrst"})).await;
    assert!(
        e.contains("not a Massachusetts city or town") && e.contains("Amherst"),
        "{e}"
    );
    let e = call_err(&h, json!({"path": "does_not_exist.csv"})).await;
    assert!(!e.is_empty());
}

#[tokio::test]
async fn an_installed_index_is_used_and_checked() {
    let h = start(|c| c.offline = true).await;
    let base = call_ok(
        &h,
        json!({"path": "address_list.csv", "write_matched_file": true}),
    )
    .await;
    // Install the bundled sample as a "full" index in /data/geocoder, one file per town.
    let text =
        std::fs::read_to_string(common::repo_root().join("snapshots/ma_geocoder_sample.idx"))
            .unwrap();
    let dir = h.data().join("geocoder");
    std::fs::create_dir_all(&dir).unwrap();
    let mut towns = serde_json::Map::new();
    for sec in text.split("#PHNGEO 1\n").filter(|s| !s.is_empty()) {
        let section = format!("#PHNGEO 1\n{sec}");
        let id = section
            .lines()
            .nth(1)
            .unwrap()
            .split('\t')
            .nth(1)
            .unwrap()
            .to_string();
        let name = format!("T{id:0>3}.idx");
        std::fs::write(dir.join(&name), &section).unwrap();
        towns.insert(
            id,
            json!({"file": name, "sha256": phn_data::snapshots::sha256_hex(section.as_bytes())}),
        );
    }
    std::fs::write(
        dir.join("manifest.json"),
        json!({"format": "phn-geocoder-index", "format_version": 1, "kind": "full", "built": "2026-11-12", "towns": towns})
            .to_string(),
    )
    .unwrap();
    let s = call_ok(&h, json!({"path": "address_list.csv"})).await;
    assert_eq!(s["data"]["index"]["kind"], "installed");
    assert_eq!(s["data"]["index"]["built"], "2026-11-12");
    assert_eq!(s["data"]["index"]["towns_covered"], 4);
    assert!(
        s["sources"]
            .to_string()
            .contains("Installed geocoder index")
    );
    assert!(!caveats(&s).contains("four-town sample"));
    assert_eq!(s["data"]["match_levels"], base["data"]["match_levels"]);
    assert_eq!(s["data"]["areas"], base["data"]["areas"]);

    // Tamper with one town file: the checksum fails, a caveat says so, and the sample is used.
    std::fs::write(dir.join("T008.idx"), "#PHNGEO 1\nT\t8\tAmherst\t015\n").unwrap();
    let s = call_ok(&h, json!({"path": "address_list.csv"})).await;
    assert!(
        caveats(&s).contains("does not match the checksum"),
        "{}",
        caveats(&s)
    );
    assert_eq!(s["data"]["match_levels"], base["data"]["match_levels"]);
}

#[tokio::test]
async fn named_columns_and_a_single_town_for_every_row() {
    let h = start(|c| {
        c.offline = true;
        c.env_suppression_threshold = Some(2);
    })
    .await;
    // Columns can be named; the town can be given once for every row.
    let s = call_ok(
        &h,
        json!({"path": "address_list.csv", "address_column": "Home Address", "town_column": "City",
               "zip_column": "ZIP", "level": "town"}),
    )
    .await;
    assert_eq!(s["data"]["level"], "town");
    assert_eq!(s["data"]["requested_level"], "town");
    let s = call_ok(&h, json!({"path": "address_list.csv", "town": "Amherst"})).await;
    assert_eq!(s["data"]["rows"], 31);
}
