//! MCP integration tests for the Phase 4 tools (public data and evidence):
//! every tool answers offline from bundled snapshots or the HTTP cache with
//! output that validates against its schema, problems the nurse can fix are
//! tool-level errors, legal or guidance answers carry the decision-support
//! notice, and local files yield aggregates only.

mod common;

use common::{everything, leaks, start, summary};
use phn_data::eutils::Eutils;
use serde_json::{Value, json};

async fn call_ok(h: &common::Harness, name: &str, args: Value) -> Value {
    let tools = h.client.list_all_tools().await.unwrap();
    let r = h.call(name, args.clone()).await;
    assert_eq!(r.is_error, Some(false), "{name} {args}: {}", summary(&r));
    let md = summary(&r);
    assert!(
        !md.trim_start().starts_with('{'),
        "{name}: first block should be markdown"
    );
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

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        common::repo_root()
            .join("crates/phn-data/tests/fixtures/phase4")
            .join(name),
    )
    .unwrap()
}

fn modes(s: &Value) -> Vec<String> {
    s["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["mode"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn community_profile_offline_cites_every_figure() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "build_community_health_profile",
        json!({"area": "Manchester by the Sea"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["area"]["name"], "Manchester-by-the-Sea");
    assert_eq!(d["acs_mode"], "snapshot");
    for i in d["indicators"].as_array().unwrap() {
        assert_eq!(i["table"].as_str().unwrap().len(), 6, "{i}");
    }
    assert!(d["chronic_disease"]["rows"].as_array().unwrap().len() >= 10);
    assert!(d["svi"]["tracts"].as_u64().unwrap() >= 1);
    let md = s["summary"].as_str().unwrap();
    assert!(md.contains("B01003") && md.contains("C17002") && md.contains("Births and deaths"));
    assert!(modes(&s).iter().all(|m| m == "snapshot" || m == "computed"));
    // Counties and tracts work too; a ZCTA is not a profile level.
    let s = call_ok(
        &h,
        "build_community_health_profile",
        json!({"area": "Hampden County", "include_chronic_disease": false}),
    )
    .await;
    assert_eq!(s["data"]["area"]["level"], "county");
    assert!(s["data"].get("chronic_disease").is_none());
    let e = call_err(
        &h,
        "build_community_health_profile",
        json!({"area": "Worchester"}),
    )
    .await;
    assert!(e.contains("Worcester"), "{e}");
    let e = call_err(&h, "build_community_health_profile", json!({})).await;
    assert!(e.contains("configure_jurisdiction"), "{e}");
}

#[tokio::test]
async fn svi_ranks_tracts() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "get_social_vulnerability_index",
        json!({"area": "Springfield", "theme": "housing_transportation", "top": 5}),
    )
    .await;
    let d = &s["data"];
    assert!(d["tracts_total"].as_u64().unwrap() > 30);
    assert_eq!(d["tracts"].as_array().unwrap().len(), 5);
    let q: u64 = d["quartile_counts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap())
        .sum();
    assert_eq!(q, d["tracts_ranked"].as_u64().unwrap());
    // A small town whose tracts belong mainly to neighbors still answers.
    let s = call_ok(
        &h,
        "get_social_vulnerability_index",
        json!({"area": "Gosnold"}),
    )
    .await;
    assert!(
        s["caveats"]
            .to_string()
            .contains("No census tract lies mainly in Gosnold")
    );
    let e = call_err(
        &h,
        "get_social_vulnerability_index",
        json!({"area": "02543"}),
    )
    .await;
    assert!(e.to_lowercase().contains("tract"), "{e}");
}

#[tokio::test]
async fn language_access_applies_the_safe_harbor_with_a_notice() {
    let h = start(|_| {}).await;
    let s = call_ok(&h, "language_access_profile", json!({"area": "Lawrence"})).await;
    let d = &s["data"];
    assert_eq!(d["threshold_persons"].as_f64().unwrap(), 1000.0);
    let spanish = d["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["group_id"] == "spanish")
        .unwrap();
    assert_eq!(spanish["threshold_status"], "clearly_meets");
    assert_eq!(d["massachusetts_top_15"].as_array().unwrap().len(), 15);
    assert!(
        s["notice"].is_string(),
        "legal decision support needs the notice"
    );
    let caveats = s["caveats"].to_string();
    assert!(caveats.contains("low confidence") && caveats.contains("not yet been reviewed"));
    // A program serving fewer people gets a lower threshold.
    let s = call_ok(
        &h,
        "language_access_profile",
        json!({"area": "Lawrence", "eligible_population": 4000}),
    )
    .await;
    assert_eq!(s["data"]["threshold_persons"].as_f64().unwrap(), 200.0);
}

#[tokio::test]
async fn disparities_inline_and_from_an_aggregated_file() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "measure_health_disparities",
        json!({"groups": [
            {"label": "Hispanic or Latino", "count": 120, "population": 90000},
            {"label": "White, not Hispanic", "count": 300, "population": 450000},
            {"label": "Black, not Hispanic", "count": 80, "population": 60000},
            {"label": "Asian, not Hispanic", "count": 20, "population": 50000}],
            "event_description": "asthma emergency visits, 2024"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["reference_group"], "Asian, not Hispanic");
    assert_eq!(d["comparisons"].as_array().unwrap().len(), 3);
    assert!(d["index_of_disparity"].as_f64().unwrap() > 0.0);
    assert!((d["attributable_fraction"].as_f64().unwrap() - 0.5).abs() < 1e-9);
    // From a file: one row per group; a small group is left out entirely.
    std::fs::write(
        h.data().join("groups.csv"),
        "Group,Cases,Population\nNorth,40,20000\nSouth,90,25000\nEast,3,4000\nWest,25,30000\n",
    )
    .unwrap();
    let s = call_ok(
        &h,
        "measure_health_disparities",
        json!({"path": "groups.csv", "reference": "named", "reference_label": "West"}),
    )
    .await;
    assert_eq!(s["data"]["suppressed_groups"], 1);
    assert!(!everything_json(&s).contains("East"));
    assert_eq!(s["privacy"]["suppression_applied"], true);
    let e = call_err(
        &h,
        "measure_health_disparities",
        json!({"groups": [{"label": "A", "count": 3, "population": 100}]}),
    )
    .await;
    assert!(e.contains("at least two"), "{e}");
    let e = call_err(
        &h,
        "measure_health_disparities",
        json!({"path": "line_list.csv"}),
    )
    .await;
    assert!(e.contains("record-level"), "{e}");
}

fn everything_json(v: &Value) -> String {
    v.to_string()
}

#[tokio::test]
async fn places_estimates_for_each_level() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "get_places_estimates",
        json!({"area": "Springfield", "measures": ["DIABETES", "Obesity"]}),
    )
    .await;
    let rows = s["data"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for r in rows {
        assert!(r["low"].as_f64().unwrap() <= r["value"].as_f64().unwrap());
        assert!(r["data_year"].as_str().unwrap().starts_with("20"));
        assert!(r["massachusetts"].is_number() && r["united_states"].is_number());
    }
    assert_eq!(s["data"]["release"], "2025");
    for (area, level) in [
        ("Hampshire County", "county"),
        ("01002", "zcta"),
        ("Pelham", "town"),
    ] {
        let s = call_ok(
            &h,
            "get_places_estimates",
            json!({"area": area, "category": "Health-Related Social Needs"}),
        )
        .await;
        assert_eq!(s["data"]["level"], level, "{area}");
        assert!(!s["data"]["rows"].as_array().unwrap().is_empty(), "{area}");
    }
    let e = call_err(
        &h,
        "get_places_estimates",
        json!({"area": "Springfield", "measures": ["NOT_A_MEASURE"]}),
    )
    .await;
    assert!(e.contains("NOT_A_MEASURE"), "{e}");
}

#[tokio::test]
async fn choropleth_maps_inline_and_file_values() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "create_choropleth_map",
        json!({"geography": "town", "title": "Pertussis rate per 100,000, 2024",
               "values": [
                   {"area": "Amherst", "value": 30.6, "count": 12},
                   {"area": "Hadley", "value": 56.3, "count": 3},
                   {"area": "Northampton", "value": 43.1, "count": 25},
                   {"area": "Belchertown", "value": 20.0, "count": 30},
                   {"area": "Hatfield", "value": 10.0, "count": 20},
                   {"area": "Pelham"}],
               "classes": 3, "value_label": "Rate per 100,000"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["suppressed_areas"], 1);
    assert_eq!(d["no_data_areas"], 1);
    assert_eq!(s["privacy"]["files_written"].as_array().unwrap().len(), 2);
    for f in d["files"].as_array().unwrap() {
        let p = h.data().join(f.as_str().unwrap());
        assert!(p.exists(), "{}", p.display());
    }
    // Hadley's rate rests on a suppressed count and is not returned.
    assert!(!everything_json(&s).contains("56.3"));
    // An aggregated file: values are drawn but not returned.
    let s = call_ok(
        &h,
        "create_choropleth_map",
        json!({"geography": "town", "title": "Cases by town", "path": "counts_by_town.csv",
               "value_is_count": true, "extent": "state"}),
    )
    .await;
    assert!(s["data"].get("areas").is_none());
    assert!(s["sources"].to_string().contains("counts_by_town.csv"));
    let e = call_err(
        &h,
        "create_choropleth_map",
        json!({"geography": "town", "title": "x", "values": [{"area": "Amherst", "value": 1}], "classes": 9}),
    )
    .await;
    assert!(e.contains("between 3 and 7"), "{e}");
    let e = call_err(
        &h,
        "create_choropleth_map",
        json!({"geography": "tract", "title": "x", "values": [{"area": "Amherst", "value": 1}]}),
    )
    .await;
    assert!(e.contains("GEOID"), "{e}");
}

#[tokio::test]
async fn pubmed_uses_the_cache_offline_and_never_sends_an_email() {
    let h = start(|_| {}).await;
    let query = "(pertussis vaccination)";
    let search = Eutils::url(
        "esearch",
        &[
            ("db", "pubmed"),
            ("term", query),
            ("retmax", "3"),
            ("sort", "relevance"),
        ],
    );
    assert!(search.contains("tool=public-health-nursing-mcp-server") && !search.contains("email"));
    let fetch = Eutils::url(
        "efetch",
        &[
            ("db", "pubmed"),
            ("id", "42309234,42317980,41759747"),
            ("retmode", "xml"),
        ],
    );
    let cache = h.ctx.cache.as_ref().expect("cache");
    cache
        .put(&search, &fixture("pubmed_esearch.xml"), 3600)
        .unwrap();
    cache
        .put(&fetch, &fixture("pubmed_efetch.xml"), 3600)
        .unwrap();
    let s = call_ok(
        &h,
        "search_pubmed",
        json!({"query": "pertussis vaccination", "max_results": 3}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["query"], query);
    assert_eq!(d["total_found"], 69);
    let arts = d["articles"].as_array().unwrap();
    assert_eq!(arts.len(), 2);
    assert_eq!(arts[0]["pmid"], "42309234");
    assert!(arts[0]["doi"].as_str().unwrap().starts_with("10."));
    assert!(
        arts[0]["url"]
            .as_str()
            .unwrap()
            .contains("pubmed.ncbi.nlm.nih.gov/42309234")
    );
    assert_eq!(modes(&s), vec!["cache"]);
    // Not cached and offline: a clear tool error.
    let e = call_err(
        &h,
        "search_pubmed",
        json!({"population": {"phrase": "school nurses"}}),
    )
    .await;
    assert!(e.contains("offline"), "{e}");
    let e = call_err(&h, "search_pubmed", json!({"max_results": 5})).await;
    assert!(e.contains("PICO"), "{e}");
}

#[tokio::test]
async fn cdc_guidance_from_cached_feeds() {
    let h = start(|_| {}).await;
    let cache = h.ctx.cache.as_ref().expect("cache");
    let search = format!(
        "{}?title=measles&max=100&sort=-datePublished",
        phn_data::cdc_guidance::SYNDICATION_MEDIA
    );
    cache
        .put(&search, &fixture("cdc_syndication_search.json"), 3600)
        .unwrap();
    cache
        .put(
            phn_data::cdc_guidance::HAN_ARCHIVE,
            &fixture("han_archive_static.json"),
            3600,
        )
        .unwrap();
    let s = call_ok(
        &h,
        "search_cdc_guidance",
        json!({"topic": "measles", "max_results": 5}),
    )
    .await;
    let items = s["data"]["items"].as_array().unwrap();
    assert!(
        items
            .iter()
            .any(|i| i["series"] == "mmwr" && i["url"].as_str().unwrap().contains("/mmwr/"))
    );
    assert!(
        items
            .iter()
            .any(|i| i["series"] == "han" && i["number"].as_str() == Some("00522"))
    );
    let searched = s["data"]["searched"].as_array().unwrap();
    assert!(
        searched
            .iter()
            .any(|x| x["series"] == "stacks" && x["status"] == "link_only")
    );
    assert!(
        s["data"]["stacks_search_url"]
            .as_str()
            .unwrap()
            .contains("stacks.cdc.gov")
    );
    assert!(s["notice"].is_string());
    let e = call_err(&h, "search_cdc_guidance", json!({"topic": "  "})).await;
    assert!(e.contains("topic"), "{e}");
    let e = call_err(
        &h,
        "search_cdc_guidance",
        json!({"topic": "measles", "since": "March 2025"}),
    )
    .await;
    assert!(e.contains("YYYY") || e.contains("2025-01-31"), "{e}");
}

#[tokio::test]
async fn local_file_tools_refuse_identifier_columns() {
    let h = start(|_| {}).await;
    for (name, args) in [
        (
            "measure_health_disparities",
            json!({"path": "line_list.csv", "group_column": "Last Name", "count_column": "Age", "population_column": "Age"}),
        ),
        (
            "create_choropleth_map",
            json!({"path": "line_list.csv", "geography": "town", "title": "x", "area_column": "Town", "value_column": "Age"}),
        ),
        (
            "create_choropleth_map",
            json!({"path": "line_list.csv", "geography": "zcta", "title": "x", "area_column": "ZIP"}),
        ),
    ] {
        let r = h.call(name, args.clone()).await;
        assert_eq!(r.is_error, Some(true), "{name} {args}");
        let found = leaks(&everything(&r), "data/line_list.csv", None);
        assert!(found.is_empty(), "{name} leaked {found:?}");
    }
}
