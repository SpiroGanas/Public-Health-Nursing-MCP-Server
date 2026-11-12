//! MCP integration tests for the Phase 15 community health assessment tools
//! (EQ-08 community_health_assessment_builder, EQ-09 prioritize_health_issues,
//! EQ-10 analyze_qualitative_data): offline calls match their output schemas,
//! errors the nurse can fix are tool errors, documents land in the output
//! folder, and nothing identifiable comes back from transcripts, CHNA files,
//! or partner rosters.

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

fn pending(s: &Value) -> bool {
    caveats(s).contains("has not yet been reviewed by the subject matter expert")
}

const FG: &str = "data/focus_group_transcript.txt";
const KII: &str = "data/key_informant_interview.txt";

// ---------------------------------------------------------------- EQ-08

#[tokio::test]
async fn assessment_for_a_town_offline_lists_the_framework_data_and_hospitals() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "community_health_assessment_builder",
        json!({"area": "Lowell"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["area"]["name"], "Lowell");
    // MAPP 2.0: three phases, three assessments, and an honest source status.
    let phases: Vec<&str> = d["framework"]["phases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        phases,
        vec![
            "Build the Community Health Improvement (CHI) Foundation",
            "Tell the Community Story",
            "Continuously Improve the Community"
        ]
    );
    assert_eq!(d["framework"]["assessments"].as_array().unwrap().len(), 3);
    assert!(
        d["framework"]["source_status"]
            .as_str()
            .unwrap()
            .contains("could not be fetched")
    );
    // Phase 3 is low confidence (its contents were not verified).
    assert_eq!(d["framework"]["phases"][2]["confidence"], "low");
    // Data sections: the offline ones are included; each other one says why not.
    let sections = d["status_assessment"]["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 8);
    let by = |id: &str| sections.iter().find(|s| s["id"] == id).unwrap();
    for id in [
        "profile",
        "language",
        "social_vulnerability",
        "environmental_justice",
    ] {
        assert_eq!(by(id)["status"], "included", "{id}: {}", by(id)["detail"]);
    }
    for sec in sections {
        let st = sec["status"].as_str().unwrap();
        assert!(["included", "unavailable"].contains(&st), "{sec}");
        if st == "unavailable" {
            assert!(sec["detail"].as_str().is_some_and(|t| !t.is_empty()));
        }
    }
    assert!(!by("profile")["headline"].as_array().unwrap().is_empty());
    // Hospitals in the town come from the CMS snapshot, with their type and ownership.
    let hospitals = d["hospital_chnas"]["hospitals"].as_array().unwrap();
    let lg = hospitals
        .iter()
        .find(|x| x["name"].as_str().unwrap().contains("Lowell General"))
        .expect("Lowell General in the list");
    assert_eq!(lg["relation"], "in the town");
    assert_eq!(lg["likely_files_community_benefits_report"], true);
    // Links to find CHNAs, and the federal and Massachusetts requirements, are cited.
    let links: Vec<&str> = d["hospital_chnas"]["directory"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["url"].as_str().unwrap())
        .collect();
    assert!(links.iter().any(|u| u.contains("massago.hylandcloud.com")));
    assert!(links.iter().all(|u| u.starts_with("https://")));
    assert!(
        d["hospital_chnas"]["federal_requirements"][0]["citation"]
            .as_str()
            .unwrap()
            .contains("1.501(r)-3")
    );
    assert!(
        d["hospital_chnas"]["reports_read"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // The draft report is in the output folder.
    let doc = d["document"].as_str().unwrap();
    assert!(
        doc.starts_with("output/community_health_assessment"),
        "{doc}"
    );
    assert!(h.data().join(doc).exists());
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert_eq!(s["privacy"]["files_written"].as_array().unwrap().len(), 1);
    // Review status, the MAPP 2.0 verification gap, and dated sources.
    assert!(pending(&s));
    assert!(caveats(&s).contains("NACCHO's own materials"));
    assert!(s["notice"].as_str().is_some());
    assert!(
        s["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["title"].as_str().unwrap().contains("1.501(r)-3"))
    );
    assert!(s["sources"].as_array().unwrap().iter().any(|x| {
        x["title"]
            .as_str()
            .unwrap()
            .contains("Hospital General Information")
            && x["mode"] == "snapshot"
    }));
}

#[tokio::test]
async fn assessment_for_a_county_skips_town_sections_and_bad_input_is_an_error() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "community_health_assessment_builder",
        json!({"area": "Hampden County", "sections": ["drinking_water", "fluoridation", "profile"],
               "write_document": false}),
    )
    .await;
    let sections = s["data"]["status_assessment"]["sections"]
        .as_array()
        .unwrap();
    for id in ["drinking_water", "fluoridation"] {
        let sec = sections.iter().find(|x| x["id"] == id).unwrap();
        assert_eq!(sec["status"], "not for this level");
    }
    assert!(s["data"]["document"].is_null());
    assert!(
        s["data"]["hospital_chnas"]["hospitals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["relation"] == "in the county")
    );
    let e = call_err(
        &h,
        "community_health_assessment_builder",
        json!({"area": "Lowell", "sections": ["weather"]}),
    )
    .await;
    assert!(e.contains("unknown section 'weather'"), "{e}");
    let e = call_err(
        &h,
        "community_health_assessment_builder",
        json!({"area": "Lowel"}),
    )
    .await;
    assert!(e.contains("Lowell"), "{e}");
    let e = call_err(&h, "community_health_assessment_builder", json!({})).await;
    assert!(
        e.contains("name a Massachusetts") || e.contains("not found"),
        "{e}"
    );
}

#[tokio::test]
async fn assessment_reads_a_pdf_chna_and_cites_the_page() {
    let h = start(|c| c.offline = true).await;
    // A fictional hospital CHNA with the priority list on page 2.
    let doc = phn_docs::model::Document {
        title: "Example General Hospital Community Health Needs Assessment 2025".into(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: None,
        logo: None,
        blocks: {
            use phn_docs::model::{Block, Inline};
            vec![
                Block::Paragraph(vec![Inline::plain(
                    "This fictional assessment describes the community served in 2025.",
                )]),
                Block::PageBreak,
                Block::Heading {
                    level: 1,
                    text: "Significant health needs".into(),
                },
                Block::List {
                    ordered: false,
                    items: vec![
                        vec![Inline::plain("Mental health and substance use")],
                        vec![Inline::plain("Housing stability")],
                        vec![Inline::plain("Access to primary care")],
                    ],
                },
                Block::PageBreak,
                Block::Paragraph(vec![Inline::plain("Appendix with methods.")]),
            ]
        },
    };
    let pdf = phn_docs::pdf::to_pdf(&doc).unwrap();
    std::fs::create_dir_all(h.data().join("chna")).unwrap();
    std::fs::write(h.data().join("chna/example_general_chna_2025.pdf"), pdf).unwrap();
    let s = call_ok(
        &h,
        "community_health_assessment_builder",
        json!({"area": "Lowell", "sections": ["fluoridation"], "chna_paths": ["chna"],
               "hospitals": ["Example General Hospital"]}),
    )
    .await;
    let reports = s["data"]["hospital_chnas"]["reports_read"]
        .as_array()
        .unwrap();
    assert_eq!(reports.len(), 1);
    let r = &reports[0];
    assert_eq!(r["pages"], 3);
    assert_eq!(r["year_seen"], 2025);
    let prio = r["priorities"].as_array().unwrap();
    let texts: Vec<&str> = prio.iter().map(|p| p["text"].as_str().unwrap()).collect();
    assert!(texts.contains(&"Housing stability"), "{texts:?}");
    assert!(
        prio.iter()
            .all(|p| p["location"].as_str().unwrap() == "page 2"),
        "{prio:?}"
    );
    let themes: Vec<&str> = s["data"]["hospital_chnas"]["common_themes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["theme"].as_str().unwrap())
        .collect();
    assert!(themes.contains(&"Housing and homelessness") && themes.contains(&"Mental health"));
    // The named hospital is added to the list to collect, flagged as not in the CMS list.
    assert!(
        s["data"]["hospital_chnas"]["hospitals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["name"] == "Example General Hospital")
    );
    assert!(caveats(&s).contains("extracted from the CHNA files by pattern"));
    assert!(
        s["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["mode"] == "local" && x["title"].as_str().unwrap().ends_with(".pdf"))
    );
}

#[tokio::test]
async fn assessment_counts_a_partner_roster_and_never_returns_the_contacts() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "community_health_assessment_builder",
        json!({"area": "Springfield", "sections": ["fluoridation"], "partners_path": "food_pantries.csv"}),
    )
    .await;
    let ro = &s["data"]["partner_assessment"]["roster"];
    assert_eq!(ro["source_file"], "food_pantries.csv");
    assert!(ro["organizations"].as_u64().unwrap() >= 4);
    assert!(ro["sectors_total"].as_u64().unwrap() >= 10);
    assert!(!ro["missing_sectors"].as_array().unwrap().is_empty());
    let all = everything(&h.call(
        "community_health_assessment_builder",
        json!({"area": "Springfield", "sections": ["fluoridation"], "partners_path": "food_pantries.csv"}),
    )
    .await);
    assert!(leaks(&all, "data/food_pantries.csv", None).is_empty());
    // The draft report lists organizations (not contact people).
    let doc = s["data"]["document"].as_str().unwrap();
    assert!(h.data().join(doc).exists());
    // A client list is refused without echoing a value.
    let e = call_err(
        &h,
        "community_health_assessment_builder",
        json!({"area": "Springfield", "sections": ["fluoridation"], "partners_path": "hrsn_screenings.csv"}),
    )
    .await;
    assert!(e.contains("list of people"), "{e}");
    assert!(leaks(&e, "data/hrsn_screenings.csv", None).is_empty());
    // A client record and a data table are refused as CHNA reports.
    let e = call_err(
        &h,
        "community_health_assessment_builder",
        json!({"area": "Springfield", "sections": ["fluoridation"], "chna_paths": ["client_note.txt"]}),
    )
    .await;
    assert!(e.contains("client record"), "{e}");
    assert!(leaks(&e, "data/client_note.txt", None).is_empty());
    let e = call_err(
        &h,
        "community_health_assessment_builder",
        json!({"area": "Springfield", "sections": ["fluoridation"], "chna_paths": ["line_list.csv"]}),
    )
    .await;
    assert!(e.contains("not a document"), "{e}");
}

// ---------------------------------------------------------------- EQ-09

#[tokio::test]
async fn prioritization_ranks_ties_missing_ratings_and_writes_a_document() {
    let h = start(|c| c.offline = true).await;
    let pearl = json!({"propriety": true, "economics": true, "acceptability": true,
                       "resources": true, "legality": true});
    let s = call_ok(
        &h,
        "prioritize_health_issues",
        json!({
            "area": "Springfield",
            "issues": [
                {"name": "Housing cost burden", "size": 8, "seriousness": 14, "effectiveness": 6,
                 "pearl": pearl, "rationale": "38% of renters pay 30% or more of income on housing."},
                {"name": "Food insecurity", "size": 5, "seriousness": 10, "effectiveness": 6, "pearl": pearl},
                {"name": "Transportation", "size": 6, "seriousness": 9, "effectiveness": 6, "pearl": pearl},
                {"name": "Opioid overdose", "size": 3, "urgency": 5, "severity": 5, "economic_loss": 3,
                 "impact_on_others": 4, "effectiveness": 7,
                 "pearl": {"propriety": true, "economics": true, "acceptability": true,
                           "resources": true, "legality": false}},
                {"name": "Dental care", "size": 4, "seriousness": 6}
            ]
        }),
    )
    .await;
    let d = &s["data"];
    let by = |n: &str| {
        d["issues"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == n)
            .unwrap()
            .clone()
    };
    // (8 + 14) x 6 / 3 = 44; (5 + 10) x 6 / 3 = 30; (6 + 9) x 6 / 3 = 30 (a tie).
    assert_eq!(by("Housing cost burden")["priority_score"], 44.0);
    assert_eq!(by("Housing cost burden")["hanlon_rank"], 1);
    assert_eq!(by("Food insecurity")["priority_score"], 30.0);
    assert_eq!(by("Food insecurity")["hanlon_rank"], 2);
    assert_eq!(by("Transportation")["hanlon_rank"], 2);
    assert_eq!(by("Food insecurity")["tied_with"][0], "Transportation");
    assert_eq!(d["ties"].as_array().unwrap().len(), 1);
    assert!(d["tie_guidance"].as_str().is_some());
    // PEARL failed (legality): the score is zero and the issue is ranked last.
    let opioid = by("Opioid overdose");
    assert_eq!(opioid["priority_score"], 0.0);
    assert_eq!(opioid["pearl_failed"][0], "Legality");
    // B = 5 + 5 + 3 + 4 = 17 and BPR = (3 + 17) x 7 / 3 = 46.67 before PEARL.
    assert_eq!(opioid["seriousness"], 17.0);
    assert_eq!(opioid["basic_priority_rating"], 46.67);
    // Missing effectiveness and PEARL: listed, not scored as zero.
    let dental = by("Dental care");
    assert_eq!(dental["hanlon_status"], "incomplete");
    assert!(dental["priority_score"].is_null());
    assert!(
        dental["missing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == "effectiveness (C)")
    );
    assert_eq!(d["ranked_count"], 4);
    assert_eq!(d["incomplete_count"], 1);
    assert!(caveats(&s).contains("missing ratings"));
    assert!(caveats(&s).contains("NACCHO's own Hanlon worksheet"));
    assert!(pending(&s));
    // Document.
    let doc = d["document"].as_str().unwrap();
    assert!(h.data().join(doc).exists());
    assert_eq!(s["privacy"]["files_written"][0], doc);
    assert!(
        s["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["title"].as_str().unwrap().contains("Hanlon"))
    );
    // PEARL as a screen: the failed issue is screened out, not ranked.
    let s2 = call_ok(
        &h,
        "prioritize_health_issues",
        json!({"pearl_mode": "prescreen", "write_document": false, "issues": [
            {"name": "A", "size": 5, "seriousness": 5, "effectiveness": 5,
             "pearl": {"propriety": true, "economics": true, "acceptability": true, "resources": true, "legality": true}},
            {"name": "B", "size": 9, "seriousness": 18, "effectiveness": 9,
             "pearl": {"propriety": false, "economics": true, "acceptability": true, "resources": true, "legality": true}}]}),
    )
    .await;
    assert_eq!(s2["data"]["screened_out_count"], 1);
    assert_eq!(s2["data"]["ranked_count"], 1);
}

#[tokio::test]
async fn prioritization_scores_a_nominal_group_vote_from_totals() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "prioritize_health_issues",
        json!({"write_document": false,
               "nominal_group": {"participants": 10, "ranks_per_participant": 3},
               "issues": [
                   {"name": "X", "rank_counts": [4, 3, 1]},
                   {"name": "Y", "rank_counts": [3, 2, 4]},
                   {"name": "Z", "rank_counts": [2, 3, 3]}]}),
    )
    .await;
    let order: Vec<&str> = s["data"]["nominal_order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    assert_eq!(order, vec!["X", "Y", "Z"]);
    let x = s["data"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "X")
        .unwrap();
    // 4 x 3 + 3 x 2 + 1 x 1 = 19
    assert_eq!(x["nominal"]["points"], 19);
    // Impossible totals are an error that says what is wrong.
    let e = call_err(
        &h,
        "prioritize_health_issues",
        json!({"write_document": false,
               "nominal_group": {"participants": 5, "ranks_per_participant": 1},
               "issues": [{"name": "A", "rank_counts": [4]}, {"name": "B", "rank_counts": [3]}]}),
    )
    .await;
    assert!(e.contains("only 5 participants"), "{e}");
}

#[tokio::test]
async fn prioritization_errors_are_tool_errors() {
    let h = start(|c| c.offline = true).await;
    let one = call_err(
        &h,
        "prioritize_health_issues",
        json!({"issues": [{"name": "Only", "size": 1}]}),
    )
    .await;
    assert!(one.contains("at least two"), "{one}");
    let dup = call_err(
        &h,
        "prioritize_health_issues",
        json!({"issues": [{"name": "Same", "size": 1}, {"name": "same", "size": 2}]}),
    )
    .await;
    assert!(dup.contains("two issues are named"), "{dup}");
    let range = call_err(
        &h,
        "prioritize_health_issues",
        json!({"issues": [{"name": "A", "size": 12}, {"name": "B", "size": 1}]}),
    )
    .await;
    assert!(range.contains("size is 12"), "{range}");
    let grid = call_err(
        &h,
        "prioritize_health_issues",
        json!({"grid_threshold": 2.0, "issues": [{"name": "A", "size": 1}, {"name": "B", "size": 1}]}),
    )
    .await;
    assert!(grid.contains("grid_threshold"), "{grid}");
}

// ---------------------------------------------------------------- EQ-10

#[tokio::test]
async fn qualitative_analysis_returns_counts_only_and_withholds_rare_themes() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "analyze_qualitative_data",
        json!({"transcript_paths": ["focus_group_transcript.txt"],
               "names_path": "participant_names.txt",
               "write_deidentified_transcripts": true}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["transcripts"].as_array().unwrap().len(), 1);
    // Five participants spoke; the facilitator's turns are left out.
    assert_eq!(d["total_participants"], 5);
    assert!(d["facilitator_turns_excluded"].as_u64().unwrap() >= 3);
    assert_eq!(d["min_voices"], 3);
    let themes: Vec<&Value> = d["themes"].as_array().unwrap().iter().collect();
    let theme = |code: &str| themes.iter().find(|t| t["code"] == code).copied();
    // Rent and housing came up for most participants.
    let housing = theme("HOUSING").expect("housing reported");
    assert!(housing["participants"].as_u64().unwrap() >= 4);
    assert_eq!(housing["transcripts"], 1);
    assert!(
        housing["summary"]
            .as_str()
            .unwrap()
            .contains("participants raised this")
    );
    // Transportation: three or more.
    assert!(theme("TRANSPORT").is_some());
    // Substance use was raised by one participant only: no row, no counts, no label.
    assert!(theme("SUBSTANCE_USE").is_none());
    assert!(d["themes_withheld"].as_u64().unwrap() >= 1);
    for t in &themes {
        let n = t["participants"].as_u64().unwrap();
        assert!(n == 0 || n >= 3, "{t}");
    }
    assert_eq!(s["privacy"]["suppression_applied"], true);
    assert_eq!(s["privacy"]["suppression_threshold"], 3);
    assert_eq!(s["privacy"]["row_level_data"], false);
    // The scrub report has counts by kind and no values.
    assert!(d["scrub"]["total_removed"].as_u64().unwrap() >= 20);
    assert!(d["scrub"]["names_from_file"].as_u64().unwrap() >= 7);
    // Nothing identifiable and no sentence from the transcript is returned.
    let all = everything(&h.call(
        "analyze_qualitative_data",
        json!({"transcript_paths": ["focus_group_transcript.txt"], "names_path": "participant_names.txt"}),
    )
    .await);
    assert!(
        leaks(&all, FG, None).is_empty(),
        "{:?}",
        leaks(&all, FG, None)
    );
    for sentence in [
        "The bus does not come after nine",
        "I have been in recovery for three years",
        "My neighbors look out for each other",
        "I cannot afford my copay",
    ] {
        assert!(!all.contains(sentence), "{sentence}");
    }
    // Files: the matrix, the summary, and the de-identified transcript, all in the output
    // folder; the de-identified transcript has no seeded identifier.
    let files: Vec<String> = d["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap().to_string())
        .collect();
    assert_eq!(files.len(), 3, "{files:?}");
    assert!(files[0].ends_with("coded_excerpt_matrix.xlsx"));
    assert!(files[1].ends_with("theme_summary.docx"));
    for f in &files {
        assert!(f.starts_with("output/qualitative_analysis"), "{f}");
        assert!(h.data().join(f).exists());
    }
    let deid = std::fs::read_to_string(h.data().join(&files[2])).unwrap();
    assert!(
        leaks(&deid, FG, None).is_empty(),
        "{:?}\n{deid}",
        leaks(&deid, FG, None)
    );
    assert!(deid.contains("P1:") && deid.contains("P5:"));
    assert_eq!(s["privacy"]["files_written"].as_array().unwrap().len(), 3);
    assert!(caveats(&s).contains("heuristic"));
    assert!(caveats(&s).contains("starter codebook"));
    assert!(pending(&s));
}

#[tokio::test]
async fn qualitative_analysis_removes_names_even_without_a_names_file() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "analyze_qualitative_data",
        json!({"transcript_paths": ["focus_group_transcript.txt", "key_informant_interview.txt"],
               "write_deidentified_transcripts": true}),
    )
    .await;
    let all = everything(&h.call(
        "analyze_qualitative_data",
        json!({"transcript_paths": ["focus_group_transcript.txt", "key_informant_interview.txt"]}),
    )
    .await);
    assert!(
        leaks(&all, FG, None).is_empty(),
        "{:?}",
        leaks(&all, FG, None)
    );
    assert!(
        leaks(&all, KII, None).is_empty(),
        "{:?}",
        leaks(&all, KII, None)
    );
    // The files hold no seeded identifier either (names come from the speaker labels, from
    // capitalization inside sentences, and from the identifier detector).
    for f in s["data"]["files"].as_array().unwrap().iter().skip(2) {
        let text = std::fs::read_to_string(h.data().join(f.as_str().unwrap())).unwrap();
        let file = if text.contains("Facilitator:") {
            FG
        } else {
            KII
        };
        let found = leaks(&text, file, None);
        assert!(found.is_empty(), "{f}: {found:?}\n{text}");
    }
    assert_eq!(s["data"]["transcripts"].as_array().unwrap().len(), 2);
    // Two transcripts: the key informant interview has one participant.
    assert_eq!(s["data"]["transcripts"][1]["participants"], 1);
}

#[tokio::test]
async fn qualitative_analysis_uses_your_codebook_and_guards_its_inputs() {
    let h = start(|c| c.offline = true).await;
    std::fs::write(
        h.data().join("my_codebook.yaml"),
        "codes:\n  - code: RENT\n    theme: Housing\n    definition: Rent and landlords\n    keywords: [rent, landlord*]\n  - code: BUS\n    definition: The bus\n    keywords: ['bus', 'buses']\n",
    )
    .unwrap();
    let s = call_ok(
        &h,
        "analyze_qualitative_data",
        json!({"transcript_paths": ["focus_group_transcript.txt"], "codebook_path": "my_codebook.yaml",
               "write_outputs": false}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["codebook"]["codes"].as_array().unwrap().len(), 2);
    assert!(
        d["codebook"]["source"]
            .as_str()
            .unwrap()
            .contains("your codebook")
    );
    let rent = d["themes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["code"] == "RENT")
        .unwrap();
    assert!(rent["participants"].as_u64().unwrap() >= 4);
    assert!(d["files"].as_array().unwrap().is_empty());
    assert!(!caveats(&s).contains("starter codebook"));
    // Problems the nurse can fix are tool errors that say what to do.
    let e = call_err(
        &h,
        "analyze_qualitative_data",
        json!({"transcript_paths": ["focus_group_transcript.txt"], "min_voices": 2}),
    )
    .await;
    assert!(e.contains("cannot be lower than 3"), "{e}");
    let e = call_err(&h, "analyze_qualitative_data", json!({})).await;
    assert!(e.contains("transcript_paths"), "{e}");
    let e = call_err(
        &h,
        "analyze_qualitative_data",
        json!({"transcript_paths": ["line_list.csv"]}),
    )
    .await;
    assert!(e.contains("not a document"), "{e}");
    assert!(leaks(&e, "data/line_list.csv", None).is_empty());
    let e = call_err(
        &h,
        "analyze_qualitative_data",
        json!({"transcript_paths": ["focus_group_transcript.txt"], "codebook_path": "line_list.csv"}),
    )
    .await;
    assert!(e.contains("not a codebook"), "{e}");
    assert!(leaks(&e, "data/line_list.csv", None).is_empty());
    let e = call_err(
        &h,
        "analyze_qualitative_data",
        json!({"transcript_paths": ["missing.txt"]}),
    )
    .await;
    assert!(e.contains("not found"), "{e}");
}

#[tokio::test]
async fn qualitative_analysis_of_a_client_note_returns_no_identifier() {
    let h = start(|c| c.offline = true).await;
    let r = h
        .call(
            "analyze_qualitative_data",
            json!({"transcript_paths": ["client_note.txt"]}),
        )
        .await;
    let all = everything(&r);
    assert!(
        leaks(&all, "data/client_note.txt", None).is_empty(),
        "{all}"
    );
}

// ---------------------------------------------------------------- documents

#[tokio::test]
async fn the_documents_pass_the_servers_own_accessibility_check() {
    let h = start(|c| c.offline = true).await;
    let mut paths: Vec<String> = Vec::new();
    let s = call_ok(
        &h,
        "community_health_assessment_builder",
        json!({"area": "Lowell", "sections": ["profile", "social_vulnerability"],
               "partners_path": "food_pantries.csv"}),
    )
    .await;
    paths.push(s["data"]["document"].as_str().unwrap().to_string());
    let s = call_ok(
        &h,
        "prioritize_health_issues",
        json!({"issues": [
            {"name": "Housing", "size": 8, "seriousness": 14, "effectiveness": 6,
             "rationale": "Renter cost burden.",
             "pearl": {"propriety": true, "economics": true, "acceptability": true, "resources": true, "legality": true}},
            {"name": "Food", "size": 5, "seriousness": 9}]}),
    )
    .await;
    paths.push(s["data"]["document"].as_str().unwrap().to_string());
    let s = call_ok(
        &h,
        "analyze_qualitative_data",
        json!({"transcript_paths": ["focus_group_transcript.txt"]}),
    )
    .await;
    let files = s["data"]["files"].as_array().unwrap();
    paths.push(files[1].as_str().unwrap().to_string());
    for path in paths {
        let r = h
            .call("check_document_accessibility", json!({"path": path}))
            .await;
        assert_eq!(r.is_error, Some(false), "{path}: {}", summary(&r));
        let report = &r.structured_content.unwrap()["data"]["report"];
        assert_eq!(report["failed"], 0, "{path}: {report}");
    }
}
