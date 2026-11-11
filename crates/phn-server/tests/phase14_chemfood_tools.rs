//! MCP integration tests for the Phase 14 chemicals and food tools
//! (ENV-08 `chemical_exposure_reference`, CHR-06 `food_access_mapper`):
//! offline calls succeed and match each tool's output schema, problems the
//! nurse can fix are tool-level errors, the community-exposure framing is in
//! every chemical answer, files land in the output folder, and local resource
//! lists come back only as counts.

mod common;

use common::{everything, leaks, start, summary};
use serde_json::{Value, json};

async fn call_ok(h: &common::Harness, name: &str, args: Value) -> Value {
    let tools = h.client.list_all_tools().await.unwrap();
    let r = h.call(name, args.clone()).await;
    assert_eq!(r.is_error, Some(false), "{name} {args}: {}", everything(&r));
    assert!(!summary(&r).trim_start().starts_with('{'));
    let structured = r.structured_content.clone().expect("structured content");
    let tool = tools.iter().find(|t| t.name == name).unwrap();
    let schema = Value::Object((**tool.output_schema.as_ref().unwrap()).clone());
    let v = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = v
        .iter_errors(&structured)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .take(5)
        .collect();
    assert!(
        errors.is_empty(),
        "{name} output does not match its schema: {errors:#?}"
    );
    structured
}

async fn call_err(h: &common::Harness, name: &str, args: Value) -> String {
    let r = h.call(name, args.clone()).await;
    assert_eq!(
        r.is_error,
        Some(true),
        "{name} {args} should fail: {}",
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

fn output_path(h: &common::Harness, display: &str) -> std::path::PathBuf {
    h.data().join(display)
}

#[tokio::test]
async fn chemical_reference_frames_limits_and_lists_contacts() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "chemical_exposure_reference",
        json!({"chemical": "toluene", "scenario": "spill_or_release"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["cas"][0], "108-88-3");
    assert_eq!(d["matched"]["method"], "exact");
    assert!(d["niosh"]["rel"].as_str().unwrap().contains("TWA"));
    assert!(!d["mrls"].as_array().unwrap().is_empty());
    assert!(
        d["toxfaqs"][0]["url"]
            .as_str()
            .unwrap()
            .starts_with("https://wwwn.cdc.gov/TSP/ToxFAQs/")
    );
    assert_eq!(d["scenario"]["id"], "spill_or_release");
    let phones: Vec<&str> = d["contacts"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["phone"].as_str())
        .collect();
    assert!(phones.contains(&"1-888-304-1133"), "{phones:?}");
    assert!(phones.contains(&"1-800-222-1222"), "{phones:?}");
    assert!(phones.contains(&"911"));
    let c = caveats(&s);
    assert!(c.contains("not safe levels for community or residential exposure"));
    assert!(c.contains("screening levels"));
    assert!(c.contains("has not yet been reviewed by the subject matter expert"));
    assert!(s["notice"].as_str().is_some_and(|n| !n.is_empty()));
    assert_eq!(s["privacy"]["row_level_data"], false);
    let srcs = s["sources"].as_array().unwrap();
    assert!(srcs.iter().all(|x| x["mode"] == "snapshot"));
    assert!(
        srcs.iter()
            .any(|x| x["url"] == "https://www.cdc.gov/niosh/npg/npgd0619.html")
    );

    // CAS lookup, and the drinking water scenario points to the water tool.
    let s = call_ok(
        &h,
        "chemical_exposure_reference",
        json!({"chemical": "127-18-4", "scenario": "drinking_water"}),
    )
    .await;
    assert_eq!(s["data"]["matched"]["method"], "cas");
    assert!(
        s["summary"]
            .as_str()
            .unwrap()
            .contains("Tetrachloroethylene")
    );
    assert!(
        s["summary"]
            .as_str()
            .unwrap()
            .contains("drinking_water_quality_lookup")
    );
}

#[tokio::test]
async fn chemical_reference_refuses_bad_and_ambiguous_names() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(
        &h,
        "chemical_exposure_reference",
        json!({"chemical": "108-88-4"}),
    )
    .await;
    assert!(e.contains("check digit"), "{e}");
    let e = call_err(
        &h,
        "chemical_exposure_reference",
        json!({"chemical": "arsenic"}),
    )
    .await;
    assert!(e.contains("more than one chemical"), "{e}");
    assert!(e.contains("inorganic"), "{e}");
    let e = call_err(&h, "chemical_exposure_reference", json!({"chemical": "  "})).await;
    assert!(e.contains("name a chemical"), "{e}");
    let e = call_err(
        &h,
        "chemical_exposure_reference",
        json!({"chemical": "qwzx plorbium"}),
    )
    .await;
    assert!(e.contains("not found"), "{e}");
}

#[tokio::test]
async fn food_access_for_a_town_writes_files_and_counts_resources_only() {
    let h = start(|c| c.offline = true).await;
    let r = h
        .call(
            "food_access_mapper",
            json!({"area": "Springfield", "pantries_path": "food_pantries.csv"}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    assert!(leaks(&everything(&r), "data/food_pantries.csv", None).is_empty());
    for org in [
        "Riverbend Example Food Pantry",
        "12 Example Street",
        "413-555-0170",
    ] {
        assert!(!everything(&r).contains(org), "{org} returned");
    }
    let s = call_ok(
        &h,
        "food_access_mapper",
        json!({"area": "Springfield", "pantries_path": "food_pantries.csv", "distance": "driving"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["area"]["name"], "Springfield");
    assert_eq!(d["distance"], "driving");
    assert_eq!(d["definitions"].as_array().unwrap().len(), 4);
    assert!(d["tracts"].as_u64().unwrap() > 20);
    let p = &d["resources"][0];
    assert_eq!(p["kind"], "pantries");
    assert_eq!(p["rows_read"], 6);
    assert_eq!(p["in_area"], 4);
    assert_eq!(p["by_town"][0]["town"], "Springfield");
    assert_eq!(p["with_coordinates"], 2);
    let kinds: Vec<&str> = p["by_type"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"Meal site") && kinds.contains(&"Mobile pantry"));
    for f in d["map_files"]
        .as_array()
        .unwrap()
        .iter()
        .chain(d["report_files"].as_array().unwrap())
    {
        assert!(output_path(&h, f.as_str().unwrap()).exists(), "{f}");
    }
    assert_eq!(d["report_files"].as_array().unwrap().len(), 2);
    assert!(
        s["privacy"]["files_written"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f.as_str().unwrap().ends_with(".docx"))
    );
    assert!(caveats(&s).contains("tool default"));
    // HIP and SNAP facts from DTA.
    let sm = s["summary"].as_str().unwrap();
    assert!(sm.contains("$40") && sm.contains("(877) 382-2363"));
}

#[tokio::test]
async fn food_access_reads_snap_retailer_and_hip_files_for_a_county() {
    let h = start(|c| c.offline = true).await;
    std::fs::write(
        h.data().join("snap_retailers.csv"),
        "Store Name,Store Type,City,State,Zip Code,Latitude,Longitude,End Date\n\
         Example Super Store,Super Store,Springfield,MA,01105,42.10,-72.59,\n\
         Example Corner Mart,Convenience Store,Holyoke,MA,01040,,,\n\
         Closed Example Market,Small Grocery Store,Chicopee,MA,01013,,,2025-03-01\n\
         Border Example Grocery,Large Grocery Store,Enfield,CT,06082,,,\n",
    )
    .unwrap();
    std::fs::write(
        h.data().join("hip_vendors.csv"),
        "Vendor,Vendor Type,Town\nExample Farm Stand,Farm stand,Westfield\nExample Market,Farmers market,Springfield\n",
    )
    .unwrap();
    let s = call_ok(
        &h,
        "food_access_mapper",
        json!({"area": "Hampden County", "snap_retailers_path": "snap_retailers.csv",
               "hip_vendors_path": "hip_vendors.csv", "definition": "half_and_ten",
               "map": false, "report": false}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["mapped_definition"], "half_and_ten");
    let snap = d["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "snap_retailers")
        .unwrap();
    assert_eq!(snap["rows_read"], 4);
    assert_eq!(snap["excluded"], 2);
    assert_eq!(snap["in_area"], 2);
    let hip = d["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "hip_vendors")
        .unwrap();
    assert_eq!(hip["in_area"], 2);
    assert!(d["files_you_can_add"].as_array().unwrap().len() == 1);
    assert!(!everything(&h.call("food_access_mapper", json!({"area": "Hampden County", "snap_retailers_path": "snap_retailers.csv", "map": false, "report": false})).await).contains("Example Super Store"));
}

#[tokio::test]
async fn food_access_refuses_client_lists_and_bad_areas() {
    let h = start(|c| c.offline = true).await;
    for arg in ["pantries_path", "snap_retailers_path", "hip_vendors_path"] {
        let mut a = json!({"area": "Springfield", "map": false, "report": false});
        a[arg] = json!("hrsn_screenings.csv");
        let e = call_err(&h, "food_access_mapper", a).await;
        assert!(e.contains("list of people"), "{e}");
        assert!(leaks(&e, "data/hrsn_screenings.csv", None).is_empty());
    }
    let e = call_err(&h, "food_access_mapper", json!({"area": "Springfeld"})).await;
    assert!(e.contains("Springfield"), "{e}");
    let e = call_err(
        &h,
        "food_access_mapper",
        json!({"area": "Springfield", "pantries_path": "no_such_file.csv"}),
    )
    .await;
    assert!(!e.is_empty());
}
