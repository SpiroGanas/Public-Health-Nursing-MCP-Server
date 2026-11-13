//! MCP integration tests for the Phase 19 law tools (POL-02 `search_public_health_law`,
//! POL-04 `draft_policy_brief`, POL-07 `health_information_sharing_guide`): outputs match their
//! schemas, citation lookups return the exact section, excerpts never exceed 25 words, index-only
//! sections say so, the optional larger corpus is used only with matching checksums, local
//! regulation files are searched with contact details masked and client-like files refused, the
//! policy brief invents nothing and counts its placeholders, the sharing guide never says that a
//! disclosure is or is not permitted, content is pending review, problems the nurse can fix are
//! tool errors, and identifiers are refused without being echoed.

mod common;

use common::{everything, start, summary};
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

fn pending(s: &Value) -> bool {
    caveats(s).contains("has not yet been reviewed by the subject matter expert")
}

fn docx_text(h: &common::Harness, display: &str) -> String {
    let path = h.data().join(display);
    assert!(path.exists(), "{display} was not written");
    let out = std::process::Command::new("unzip")
        .args(["-p", path.to_str().unwrap(), "word/document.xml"])
        .output()
        .expect("unzip");
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn words(s: &str) -> usize {
    s.split_whitespace().count()
}

fn citations(s: &Value) -> Vec<String> {
    s["data"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["citation"].as_str().unwrap().to_string())
        .collect()
}

// ---------------------------------------------------------------- POL-02

#[tokio::test]
async fn a_citation_returns_the_exact_section_with_dates_link_and_a_short_excerpt() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "search_public_health_law", json!({"query": "105 CMR 300.200"})).await;
    let d = &s["data"];
    assert_eq!(d["interpretation"], "citation lookup");
    let hits = d["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert_eq!(hit["citation"], "105 CMR 300.200");
    assert_eq!(hit["title"], "Isolation and Quarantine Requirements");
    assert_eq!(hit["coverage"], "full_text");
    assert_eq!(hit["chapter_date"], "2022-05-27");
    assert_eq!(hit["retrieved"], "2026-11-12");
    assert!(hit["url"].as_str().unwrap().starts_with("https://www.mass.gov/regulations/105-CMR-30000"));
    assert!(hit["pdf_url"].as_str().unwrap().contains("105-cmr-300"));
    let ex = hit["excerpt"].as_str().unwrap();
    assert!(words(ex) <= 25, "{ex}");
    // Sources carry dates and mode; the content is pending review; a legal notice is present.
    let src = &s["sources"][0];
    assert_eq!(src["mode"], "snapshot");
    assert_eq!(src["retrieved"], "2026-11-12");
    assert!(pending(&s));
    assert!(s["notice"].as_str().is_some());
    assert!(caveats(&s).contains("not legal advice"));
    assert_eq!(s["privacy"]["row_level_data"], false);
}

#[tokio::test]
async fn statute_citations_in_several_forms_find_the_section() {
    let h = start(|c| c.offline = true).await;
    for q in [
        "M.G.L. c. 111, § 95",
        "MGL c.111 s.95",
        "chapter 111 section 95",
        "G.L. c. 111 sec. 95",
    ] {
        let s = call_ok(&h, "search_public_health_law", json!({"query": q})).await;
        assert_eq!(citations(&s), vec!["M.G.L. c. 111, § 95".to_string()], "{q}");
        let hit = &s["data"]["hits"][0];
        assert_eq!(hit["coverage"], "full_text");
        assert!(hit["url"].as_str().unwrap().ends_with("/Chapter111/Section95"));
        assert!(words(hit["excerpt"].as_str().unwrap()) <= 25);
    }
}

#[tokio::test]
async fn index_only_sections_say_so_and_have_no_excerpt() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "search_public_health_law", json!({"query": "M.G.L. c. 111, § 143"})).await;
    let hit = &s["data"]["hits"][0];
    assert_eq!(hit["citation"], "M.G.L. c. 111, § 143");
    assert_eq!(hit["coverage"], "index_only");
    assert!(hit["excerpt"].is_null());
    assert!(hit["url"].as_str().unwrap().contains("Section143"));
    assert!(caveats(&s).contains("index-only"));
    // full_text_only skips it.
    let s = call_ok(
        &h,
        "search_public_health_law",
        json!({"query": "M.G.L. c. 111, § 143", "full_text_only": true}),
    )
    .await;
    assert!(s["data"]["hits"].as_array().unwrap().is_empty());
    // A chapter that is index-only in 105 CMR lists titles without text.
    let s = call_ok(&h, "search_public_health_law", json!({"query": "105 CMR 130"})).await;
    let hits = s["data"]["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|x| x["coverage"] == "index_only" && x["excerpt"].is_null()));
}

#[tokio::test]
async fn a_chapter_citation_lists_its_sections_and_respects_max_results() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "search_public_health_law",
        json!({"query": "105 CMR 410", "max_results": 5}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["interpretation"], "chapter lookup");
    assert_eq!(d["hits"].as_array().unwrap().len(), 5);
    assert!(d["more_hits"].as_u64().unwrap() > 20);
    assert_eq!(d["hits"][0]["citation"], "105 CMR 410.001");
}

#[tokio::test]
async fn keyword_search_finds_the_core_provisions_and_every_excerpt_is_short() {
    let h = start(|c| c.offline = true).await;
    let queries = [
        ("board of health order to isolate or quarantine a person with a dangerous disease", &["105 CMR 300.200", "105 CMR 300.210", "M.G.L. c. 111, § 95", "M.G.L. c. 111, § 104"][..]),
        ("confidentiality of reports of disease", &["105 CMR 300.120"][..]),
        ("lead poisoning mandatory blood lead screening", &["105 CMR 460.050", "105 CMR 460.040"][..]),
        ("delegation to unlicensed persons by a nurse", &["244 CMR 3.05"][..]),
        ("minimum age tobacco sale", &["105 CMR 665.010", "105 CMR 665.005", "105 CMR 665.020"][..]),
    ];
    for (q, expect) in queries {
        let s = call_ok(&h, "search_public_health_law", json!({"query": q, "max_results": 10})).await;
        let got = citations(&s);
        assert!(
            expect.iter().any(|e| got.iter().any(|g| g == e)),
            "{q}: expected one of {expect:?} in {got:?}"
        );
        for hit in s["data"]["hits"].as_array().unwrap() {
            if let Some(ex) = hit["excerpt"].as_str() {
                assert!(words(ex) <= 25, "{q}: {ex}");
            }
            assert!(hit["retrieved"].as_str().unwrap().starts_with("2026-"));
            assert!(hit["url"].is_string());
        }
    }
    // Topic starting points resolve to real sections with titles.
    let s = call_ok(&h, "search_public_health_law", json!({"query": "isolation and quarantine orders"})).await;
    let topics = s["data"]["topics"].as_array().unwrap();
    assert!(topics.iter().any(|t| t["id"] == "isolation_quarantine"));
    let start_with = topics[0]["start_with"].as_array().unwrap();
    assert!(start_with.iter().all(|x| x.as_str().unwrap().contains('(')));
}

#[tokio::test]
async fn the_244_cmr_hits_point_to_the_scope_of_practice_guide() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "search_public_health_law", json!({"query": "244 CMR 3.05"})).await;
    let hit = &s["data"]["hits"][0];
    assert_eq!(hit["citation"], "244 CMR 3.05");
    assert_eq!(hit["coverage"], "full_text");
    let rel = hit["related"].as_array().unwrap();
    assert!(
        rel.iter().any(|r| r.as_str().unwrap().contains("nursing_scope_of_practice_guide")),
        "{rel:?}"
    );
    let s = call_ok(
        &h,
        "search_public_health_law",
        json!({"query": "delegation", "sources": ["cmr_244"]}),
    )
    .await;
    assert!(s["data"]["hits"].as_array().unwrap().iter().all(|x| x["source"] == "cmr_244"));
}

#[tokio::test]
async fn coverage_says_what_is_bundled_in_full_text_and_what_is_not() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "search_public_health_law", json!({"query": "105 CMR 300.001"})).await;
    let cov = s["data"]["coverage"].as_array().unwrap();
    let by = |k: &str| cov.iter().find(|c| c["source"] == k).unwrap();
    assert_eq!(by("mgl_c111")["sections"], 510);
    assert_eq!(by("mgl_c111")["full_text_sections"], 45);
    assert_eq!(by("cmr_105")["chapters"], 89);
    assert_eq!(by("cmr_244")["chapters"], 8);
    assert_eq!(by("cmr_244")["index_only_sections"], 0);
    assert!(by("cmr_105")["index_only_sections"].as_u64().unwrap() > 2000);
    assert_eq!(by("cmr_105")["origin"], "bundled");
    assert!(!by("cmr_105")["not_included"].as_array().unwrap().is_empty());
    assert!(by("mgl_c111")["coverage"].as_str().unwrap().contains("index-only"));
}

#[tokio::test]
async fn problems_the_nurse_can_fix_are_errors_and_identifiers_are_not_echoed() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(&h, "search_public_health_law", json!({"query": "   "})).await;
    assert!(e.contains("query"));
    let e = call_err(
        &h,
        "search_public_health_law",
        json!({"query": "quarantine order for the family at 837 Elm Street, phone 413-555-0100"}),
    )
    .await;
    assert!(!e.contains("413-555-0100") && !e.contains("837 Elm"), "{e}");
    let e = call_err(&h, "search_public_health_law", json!({"query": "x".repeat(400)})).await;
    assert!(e.contains("too long"));
    let e = call_err(
        &h,
        "search_public_health_law",
        json!({"query": "quarantine", "local_folder": "line_list.csv"}),
    )
    .await;
    assert!(e.contains("folder"), "{e}");
    let e = call_err(
        &h,
        "search_public_health_law",
        json!({"query": "quarantine", "local_folder": "no_such_folder"}),
    )
    .await;
    assert!(e.contains("not found") || e.contains("was not found"), "{e}");
}

#[tokio::test]
async fn local_regulations_are_searched_with_contacts_masked_and_client_files_refused() {
    let h = start(|c| c.offline = true).await;
    let dir = h.data().join("local_regulations");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("tobacco_regulation.md"),
        "# Section 2: Sale of flavored tobacco products\n\
         No retailer in the town shall sell a flavored tobacco product. The board of health may \
         suspend a permit after a hearing. Questions go to the board clerk at 413-555-0199 or \
         clerk@town.example.org.\n",
    )
    .unwrap();
    // A client-record-like file in the same folder: a date of birth and a record number.
    std::fs::write(
        dir.join("visit_note.txt"),
        "Client visit note for Maria Garcia, DOB 03/02/2015, MRN: 4455667. Flavored tobacco \
         counseling given at 12 Pine Street.\n",
    )
    .unwrap();
    std::fs::write(dir.join("line_list.csv"), "name,dob\nA B,2010-01-01\n").unwrap();
    let s = call_ok(
        &h,
        "search_public_health_law",
        json!({"query": "flavored tobacco permit suspension", "sources": ["local"]}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["local"]["searched"], true);
    assert_eq!(d["local"]["files_indexed"], 1);
    assert_eq!(d["local"]["refused_client_like_files"], 1);
    assert_eq!(d["local"]["tables_ignored"], 1);
    let hits = d["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    for hit in hits {
        assert_eq!(hit["source"], "local");
        assert_eq!(hit["coverage"], "local_file");
        assert!(words(hit["excerpt"].as_str().unwrap()) <= 25);
    }
    let all = everything(&h.call(
        "search_public_health_law",
        json!({"query": "flavored tobacco permit suspension", "sources": ["local"]}),
    )
    .await);
    for secret in ["413-555-0199", "clerk@town.example.org", "Maria Garcia", "03/02/2015", "4455667", "12 Pine Street"] {
        assert!(!all.contains(secret), "{secret} leaked: {all}");
    }
    assert!(!all.contains("visit_note"), "refused files are never named");
    assert!(s["privacy"]["notes"].to_string().contains("client"));
    // The state law is searched alongside by default and local hits are marked.
    let s = call_ok(&h, "search_public_health_law", json!({"query": "flavored tobacco"})).await;
    let srcs: Vec<&str> = s["data"]["hits"].as_array().unwrap().iter().map(|x| x["source"].as_str().unwrap()).collect();
    assert!(srcs.contains(&"cmr_105") || srcs.contains(&"local"));
    // Without the folder, the tool says only state law was searched.
    let h2 = start(|c| c.offline = true).await;
    let s = call_ok(&h2, "search_public_health_law", json!({"query": "tobacco"})).await;
    assert_eq!(s["data"]["local"]["searched"], false);
}

#[tokio::test]
async fn a_larger_corpus_is_used_only_when_its_checksums_match() {
    let h = start(|c| c.offline = true).await;
    // Section 143 is index-only in the bundled corpus.
    let law = h.data().join("law_corpus");
    std::fs::create_dir_all(&law).unwrap();
    let corpus = json!({
        "format": "phn-law-corpus", "format_version": 1, "source_id": "mgl_c111",
        "title": "x", "kind": "statute", "retrieved": "2026-11-20", "index_url": "https://example.org",
        "chapters": [],
        "provisions": [{"c": "M.G.L. c. 111, § 143", "n": "143", "ch": "111",
            "t": "Trade or employment attended with noisome and injurious odors",
            "u": "https://malegislature.gov/Laws/GeneralLaws/PartI/TitleXVI/Chapter111/Section143",
            "x": "Extended corpus text of section one hundred forty-three about odors."}]
    })
    .to_string();
    std::fs::write(law.join("c111.json"), &corpus).unwrap();
    let sum = String::from_utf8(
        std::process::Command::new("sha256sum")
            .arg(law.join("c111.json"))
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let sum = sum.split_whitespace().next().unwrap().to_string();
    let write_manifest = |sha: &str| {
        std::fs::write(
            law.join("manifest.json"),
            json!({"format": "phn-law-corpus-manifest", "format_version": 1, "built": "2026-11-20",
                   "files": [{"path": "c111.json", "source_id": "mgl_c111", "sha256": sha}]})
            .to_string(),
        )
        .unwrap();
    };
    write_manifest("0000");
    let s = call_ok(&h, "search_public_health_law", json!({"query": "M.G.L. c. 111, § 143"})).await;
    assert_eq!(s["data"]["hits"][0]["coverage"], "index_only");
    assert!(caveats(&s).contains("checksum"), "{}", caveats(&s));
    write_manifest(&sum);
    let s = call_ok(&h, "search_public_health_law", json!({"query": "M.G.L. c. 111, § 143"})).await;
    assert_eq!(s["data"]["hits"][0]["coverage"], "full_text");
    assert!(s["data"]["hits"][0]["excerpt"].as_str().unwrap().contains("odors"));
    let cov = s["data"]["coverage"].as_array().unwrap();
    assert_eq!(cov.iter().find(|c| c["source"] == "mgl_c111").unwrap()["origin"], "extended");
    assert!(caveats(&s).contains("Larger law corpus in use"));
}

// ---------------------------------------------------------------- POL-04

#[tokio::test]
async fn an_empty_brief_is_a_template_of_placeholders_with_the_eight_steps() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "draft_policy_brief", json!({})).await;
    let d = &s["data"];
    assert_eq!(d["length"], "one_page");
    let steps = d["eightfold_path"].as_array().unwrap();
    assert_eq!(steps.len(), 8);
    assert_eq!(steps[0]["name"], "Define the problem");
    assert_eq!(steps[7]["name"], "Tell your story");
    assert!(steps.iter().all(|x| x["status"] != "supplied"));
    assert_eq!(d["criteria"].as_array().unwrap().len(), 4);
    assert_eq!(d["comparison"]["rows"].as_array().unwrap().len(), 3);
    assert_eq!(d["comparison"]["accessible"], true);
    assert!(d["placeholders_remaining"].as_u64().unwrap() >= 10);
    assert_eq!(d["readability"]["checked"], false);
    assert!(pending(&s));
    // Sources: the open-access articles and abstracts, with dates; Bardach's book was not read.
    let titles = s["sources"].to_string();
    assert!(titles.contains("Namibia") || titles.contains("disability policy"));
    assert!(caveats(&s).contains("Bardach's book was not read"));
    let files = d["document_files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    let text = docx_text(&h, files[0].as_str().unwrap());
    assert!(text.contains("[NURSE: policy brief title]"));
    assert!(text.contains("[NURSE: problem statement"));
    assert!(text.contains("Options compared against the criteria"));
    assert_eq!(s["privacy"]["files_written"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_filled_brief_uses_only_what_the_nurse_supplied() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "draft_policy_brief",
        json!({
            "title": "Mold in rental housing",
            "length": "full",
            "audience": "board_of_health",
            "problem": "Many renters report mold in their homes. Mold can make asthma worse. The board has no rule that sets a standard for repairs.",
            "evidence": [
                {"finding": "Our 2025 complaint log shows mold as the most common housing complaint", "citation": "Health department complaint log, 2025", "strength": "local data"},
                {"finding": "Indoor dampness is linked to asthma in children"}
            ],
            "options": [
                {"name": "Adopt a mold repair rule", "description": "The board adopts a rule that sets a repair deadline for mold."},
                {"name": "Education only", "description": "Staff give landlords and renters a guide."},
                {"name": "Continue the current approach"}
            ],
            "criteria": ["health_impact", "feasibility", "Community trust"],
            "ratings": [
                {"option": "Adopt a mold repair rule", "criterion": "health_impact", "rating": "strong", "reason": "Sets a clear standard"},
                {"option": "Education only", "criterion": "feasibility", "rating": "strong", "reason": "Uses current staff"},
                {"option": "Continue the current approach", "criterion": "Community trust", "rating": "weak"}
            ],
            "request": "Decide whether to draft a rule for a hearing.",
            "recommendation": "Draft a rule and hold a hearing in the spring.",
            "output": "both"
        }),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["length"], "full");
    assert_eq!(d["audience"], "Board of health");
    let sections: Vec<&str> = d["sections"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert!(sections.contains(&"criteria") && sections.contains(&"tradeoffs") && sections.contains(&"references"));
    let status = |id: &str| d["sections"].as_array().unwrap().iter().find(|x| x["id"] == id).unwrap()["status"].as_str().unwrap().to_string();
    assert_eq!(status("problem"), "supplied");
    assert_eq!(status("evidence"), "partly supplied", "one item has no citation");
    assert_eq!(status("options"), "partly supplied", "one option has no description");
    assert_eq!(status("tradeoffs"), "placeholder");
    let m = &d["comparison"];
    assert_eq!(m["criteria"], json!(["Health impact", "Implementation feasibility", "Community trust"]));
    assert_eq!(m["rows"][0]["cells"][0], "Strong: Sets a clear standard");
    assert_eq!(m["rows"][1]["cells"][1], "Strong: Uses current staff");
    assert!(m["rows"][2]["cells"][2].as_str().unwrap().starts_with("Weak: [NURSE:"));
    assert!(m["rows"][0]["cells"][1].as_str().unwrap().contains("[NURSE: rating"));
    let r = &d["readability"];
    assert_eq!(r["checked"], true, "{r}");
    assert!(r["median_grade"].as_f64().unwrap() > 0.0);
    // Custom criteria are labeled as the nurse's own; suggestions are flagged.
    let crit = d["criteria"].as_array().unwrap();
    assert_eq!(crit[2]["origin"], "your own criterion");
    // Two files: Word and Markdown. Nothing in them comes from outside the arguments.
    let files = d["document_files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    let md_file = files.iter().find(|f| f.as_str().unwrap().ends_with(".md")).unwrap();
    let md = std::fs::read_to_string(h.data().join(md_file.as_str().unwrap())).unwrap();
    assert!(md.contains("Many renters report mold in their homes."));
    assert!(md.contains("Source: Health department complaint log, 2025."));
    assert!(md.contains("[NURSE: citation for this finding]"));
    assert!(md.contains("| Option |") && md.contains("Strong: Sets a clear standard"));
    assert!(md.contains("Bardach, 2012, as cited in"));
    let docx = files.iter().find(|f| f.as_str().unwrap().ends_with(".docx")).unwrap();
    assert!(docx_text(&h, docx.as_str().unwrap()).contains("Mold in rental housing"));
}

#[tokio::test]
async fn one_page_leaves_out_the_long_sections_and_output_none_writes_no_file() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "draft_policy_brief", json!({"output": "none"})).await;
    let ids: Vec<&str> = s["data"]["sections"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"matrix") && ids.contains(&"recommendation"));
    assert!(!ids.contains(&"criteria") && !ids.contains(&"tradeoffs") && !ids.contains(&"references"));
    assert!(s["data"]["document_files"].as_array().unwrap().is_empty());
    assert!(s["privacy"]["files_written"].as_array().unwrap().is_empty());
    let steps = s["data"]["eightfold_path"].as_array().unwrap();
    assert_eq!(steps[3]["status"], "not in this length");
}

#[tokio::test]
async fn brief_problems_are_tool_errors_and_identifiers_are_refused_without_echo() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(
        &h,
        "draft_policy_brief",
        json!({"options": [{"name": "A"}], "ratings": [{"option": "B", "criterion": "equity", "rating": "mixed"}]}),
    )
    .await;
    assert!(e.contains("option"), "{e}");
    let e = call_err(
        &h,
        "draft_policy_brief",
        json!({"options": [{"name": "A"}], "ratings": [{"option": "A", "criterion": "nonexistent", "rating": "mixed"}]}),
    )
    .await;
    assert!(e.contains("criterion"), "{e}");
    let e = call_err(&h, "draft_policy_brief", json!({"options": [{"name": "A"}, {"name": "a"}]})).await;
    assert!(e.contains("same name"));
    let e = call_err(&h, "draft_policy_brief", json!({"problem": "The family at 837 Elm Street, 413-555-0100 has mold"})).await;
    assert!(!e.contains("413-555-0100") && !e.contains("837 Elm"), "{e}");
    let e = call_err(&h, "draft_policy_brief", json!({"evidence": [{"finding": "Email maria.garcia@example.org about it"}]})).await;
    assert!(!e.contains("maria.garcia@example.org"), "{e}");
    let seven: Vec<Value> = (0..7).map(|i| json!({"name": format!("Option {i}")})).collect();
    let e = call_err(&h, "draft_policy_brief", json!({"options": seven})).await;
    assert!(e.contains("at most"));
}

// ---------------------------------------------------------------- POL-07

fn regime<'a>(s: &'a Value, id: &str) -> &'a Value {
    s["data"]["regimes"].as_array().unwrap().iter().find(|r| r["id"] == id).unwrap()
}

fn provision_ids(r: &Value) -> Vec<String> {
    r["provisions"].as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap().to_string()).collect()
}

#[tokio::test]
async fn with_no_answers_the_guide_asks_the_structured_questions() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "health_information_sharing_guide", json!({})).await;
    let d = &s["data"];
    assert_eq!(d["answer_type"], "governing_rules_and_cautions");
    assert_eq!(d["status"], "needs_answers");
    let q: Vec<&str> = d["questions"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert_eq!(q, ["holder_role", "record_type", "recipient", "purpose", "consent"]);
    assert!(d["questions"].as_array().unwrap().iter().all(|x| x["answer"].is_null() && !x["choices"].as_array().unwrap().is_empty()));
    let regs = d["regimes"].as_array().unwrap();
    assert_eq!(regs.len(), 6);
    assert!(regs.iter().all(|r| r["bears_on_your_situation"] == "unassessed" && r["provisions"].as_array().unwrap().is_empty()));
    assert!(pending(&s));
    assert!(s["notice"].as_str().is_some());
    assert!(d["coverage"]["partial"].as_bool().unwrap());
    assert!(d["coverage"]["not_read"].to_string().contains("603 CMR 23.00"));
}

#[tokio::test]
async fn a_school_nurse_health_record_is_ferpa_not_hipaa() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "health_information_sharing_guide",
        json!({"holder_role": "school_nurse_public", "record_type": "school_health_record",
               "recipient": "public_health_authority", "purpose": "disease_investigation"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["status"], "rules_identified");
    assert_eq!(regime(&s, "ferpa")["bears_on_your_situation"], "likely_governs");
    let hipaa = regime(&s, "hipaa");
    assert_eq!(hipaa["bears_on_your_situation"], "not_indicated");
    assert_eq!(provision_ids(hipaa), ["hipaa_phi_excludes_ferpa"]);
    let ferpa = provision_ids(regime(&s, "ferpa"));
    assert!(ferpa.contains(&"ferpa_consent".to_string()) && ferpa.contains(&"ferpa_education_record".to_string()), "{ferpa:?}");
    assert_eq!(regime(&s, "ma_student_records")["bears_on_your_situation"], "likely_governs");
    assert_eq!(regime(&s, "part2")["bears_on_your_situation"], "check");
    // Regimes that bear on the situation come first.
    assert_eq!(d["regimes"][0]["bears_on_your_situation"], "likely_governs");
    assert!(!d["facts_to_confirm"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn immunization_records_to_a_school_bring_in_the_hipaa_school_permission_and_the_state_rule() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "health_information_sharing_guide",
        json!({"holder_role": "health_care_provider", "record_type": "immunization_record",
               "recipient": "school", "purpose": "school_immunization", "consent": "verbal",
               "subject_is_minor": true}),
    )
    .await;
    let hipaa = regime(&s, "hipaa");
    assert_eq!(hipaa["bears_on_your_situation"], "likely_governs");
    let ids = provision_ids(hipaa);
    assert!(ids.contains(&"hipaa_school_immunization".to_string()), "{ids:?}");
    assert!(hipaa["cautions"].to_string().contains("Minimum necessary"));
    assert!(hipaa["cautions"].to_string().contains("minor"));
    let ma = regime(&s, "ma_health_confidentiality");
    assert!(provision_ids(ma).contains(&"ma_cmr_300_191".to_string()));
    // The school side: FERPA may apply once the school keeps the record.
    assert_eq!(regime(&s, "ferpa")["bears_on_your_situation"], "may_apply");
}

#[tokio::test]
async fn substance_use_records_and_hiv_results_carry_their_own_cautions() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "health_information_sharing_guide",
        json!({"holder_role": "substance_use_program", "record_type": "substance_use_treatment_record",
               "recipient": "public_health_authority", "purpose": "public_health_reporting", "consent": "none"}),
    )
    .await;
    let p2 = regime(&s, "part2");
    assert_eq!(p2["bears_on_your_situation"], "likely_governs");
    assert!(provision_ids(p2).contains(&"part2_public_health".to_string()));
    let c = p2["cautions"].to_string();
    assert!(c.contains("de-identified") && c.contains("2.31"));
    // HIV results held by a health department.
    let s = call_ok(
        &h,
        "health_information_sharing_guide",
        json!({"holder_role": "local_health_department", "record_type": "hiv_test_result",
               "recipient": "public_requester", "purpose": "public_records_request"}),
    )
    .await;
    let ma = regime(&s, "ma_health_confidentiality");
    assert_eq!(ma["bears_on_your_situation"], "likely_governs");
    assert!(provision_ids(ma).contains(&"ma_c111_70f".to_string()));
    assert!(ma["cautions"].to_string().contains("written informed consent"));
    let pr = regime(&s, "ma_public_records");
    assert_eq!(pr["bears_on_your_situation"], "may_apply");
    assert!(pr["cautions"].to_string().contains("10 business days"));
    assert_eq!(regime(&s, "hipaa")["bears_on_your_situation"], "may_apply");
}

#[tokio::test]
async fn the_guide_never_gives_a_yes_or_no_and_every_quote_is_short_and_dated() {
    let h = start(|c| c.offline = true).await;
    let combos = [
        json!({"holder_role": "health_care_provider", "record_type": "clinical_record", "recipient": "law_enforcement_or_court", "purpose": "legal_process"}),
        json!({"holder_role": "local_health_department", "record_type": "communicable_disease_report", "recipient": "exposed_person", "purpose": "disease_investigation"}),
        json!({"holder_role": "school_nurse_private", "record_type": "school_health_record", "recipient": "individual_or_parent", "purpose": "treatment_or_care"}),
        json!({"holder_role": "state_agency", "record_type": "administrative_record", "recipient": "researcher", "purpose": "research"}),
        json!({"holder_role": "other", "record_type": "other", "purpose": "emergency_threat"}),
    ];
    for a in combos {
        let s = call_ok(&h, "health_information_sharing_guide", a.clone()).await;
        let all = everything(&h.call("health_information_sharing_guide", a.clone()).await).to_lowercase();
        for banned in ["you may share", "you may disclose", "is permitted", "is not permitted", "you can share", "you cannot share", "you must not share"] {
            assert!(!all.contains(banned), "{a}: contains '{banned}'");
        }
        assert_eq!(s["data"]["answer_type"], "governing_rules_and_cautions");
        assert!(caveats(&s).contains("never says that a disclosure is or is not permitted"));
        for r in s["data"]["regimes"].as_array().unwrap() {
            for p in r["provisions"].as_array().unwrap() {
                assert!(words(p["quote"].as_str().unwrap()) <= 25, "{}", p["id"]);
                assert!(p["url"].as_str().unwrap().starts_with("https://"));
                assert!(p["source"].as_str().unwrap().len() > 5);
            }
        }
        for src in s["sources"].as_array().unwrap() {
            assert!(src["retrieved"].is_string() && src["url"].is_string());
        }
        assert_eq!(s["privacy"]["row_level_data"], false);
    }
}

#[tokio::test]
async fn sharing_guide_refuses_identifiers_in_free_text_without_echo() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(
        &h,
        "health_information_sharing_guide",
        json!({"situation": "The student at 837 Elm Street, phone 413-555-0100, born 03/02/2015"}),
    )
    .await;
    assert!(!e.contains("413-555-0100") && !e.contains("837 Elm") && !e.contains("03/02/2015"), "{e}");
    let s = call_ok(
        &h,
        "health_information_sharing_guide",
        json!({"holder_role": "school_nurse_public", "record_type": "school_health_record",
               "situation": "A coach asks for a student's asthma plan"}),
    )
    .await;
    assert_eq!(s["data"]["status"], "partial");
    assert!(summary_of(&s).contains("A coach asks for"));
}

fn summary_of(s: &Value) -> String {
    s["summary"].as_str().unwrap_or_default().to_string()
}
