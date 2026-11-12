//! MCP integration tests for the Phase 18 strategy tools (LEAD-03
//! `assess_foundational_public_health_services`, LEAD-04 `phab_accreditation_crosswalk`, LEAD-05
//! `build_strategic_plan`): offline calls succeed and match their output schemas, the
//! content-pending caveat is shown, problems the nurse can fix are tool errors, files land in the
//! output folder, client files and transcripts are never used, and nothing from a worksheet's
//! contact columns comes back.

mod common;

use common::{everything, leaks, start, summary};
use serde_json::{Value, json};

const FPHS: &str = "assess_foundational_public_health_services";
const PHAB: &str = "phab_accreditation_crosswalk";
const PLAN: &str = "build_strategic_plan";

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
    let p = h.data().join(display);
    assert!(p.exists(), "{display} was not written");
    phn_local::docx::read_docx(&p)
        .expect("readable DOCX")
        .plain_text()
}

#[tokio::test]
async fn the_tools_are_listed_in_the_leadership_toolset_with_long_descriptions() {
    let h = start(|c| c.offline = true).await;
    let tools = h.client.list_all_tools().await.unwrap();
    for n in [FPHS, PHAB, PLAN] {
        let t = tools
            .iter()
            .find(|t| t.name == n)
            .unwrap_or_else(|| panic!("{n}"));
        assert!(t.output_schema.is_some(), "{n}");
        assert!(t.description.as_ref().unwrap().len() > 400, "{n}");
        assert_eq!(
            phn_tools::registry::tool(n).unwrap().toolset,
            phn_core::Toolset::Leadership
        );
    }
}

#[tokio::test]
async fn fphs_writes_a_worksheet_first_and_never_rates_anything() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, FPHS, json!({"town": "Amherst"})).await;
    let d = &s["data"];
    assert_eq!(d["mode"], "worksheet");
    assert_eq!(d["standards_in_scope"], 166);
    assert_eq!(d["total_gaps"], 0);
    for set in d["sets"].as_array().unwrap() {
        assert_eq!(set["not_rated"], set["standards"], "{set}");
        assert!(set["percent_met_of_rated"].is_null());
    }
    assert_eq!(
        d["shared_services"]["arrangement"]["arrangement"],
        "Hampshire Public Health Shared Services Collaborative"
    );
    assert!(pending(&s), "{}", caveats(&s));
    assert!(caveats(&s).contains("SAPHE 2.0"));
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert_eq!(s["privacy"]["files_written"].as_array().unwrap().len(), 2);
    // Sources show dates and the snapshot mode.
    let sources = s["sources"].as_array().unwrap();
    assert!(
        sources
            .iter()
            .any(|x| x["mode"] == "snapshot" && x["retrieved"].is_string())
    );
    // The worksheet is a workbook with the Self-assessment table first, and the guide says
    // the tool never fills in an answer.
    let files: Vec<String> = d["files"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap().to_string())
        .collect();
    let xlsx = files.iter().find(|f| f.ends_with(".xlsx")).unwrap();
    let wb = phn_local::table::read_tables(&h.data().join(xlsx)).unwrap();
    assert_eq!(wb[0].name, "Self-assessment");
    assert_eq!(wb[0].row_count(), 166);
    let status = wb[0].column_index("Status").unwrap();
    assert!(wb[0].column(status).all(|v| v.trim().is_empty()));
    let docx = files.iter().find(|f| f.ends_with(".docx")).unwrap();
    assert!(docx_text(&h, docx).contains("never fills") || docx_text(&h, docx).contains("does not fill") || docx_text(&h, docx).contains("only organizes"));
}

#[tokio::test]
async fn fphs_gap_analysis_from_typed_statuses_and_a_filled_worksheet() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        FPHS,
        json!({"statuses": [
            {"standard_id": "PS-DIS-002", "status": "not_met", "delivered_by": "shared",
             "note": "Private note for the file"},
            {"standard_id": "PS-DIS-001", "status": "met"},
            {"standard_id": "FPHS-07.01", "status": "partial"}]}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["mode"], "gap_analysis");
    assert_eq!(d["total_gaps"], 2);
    let first = &d["priority_gaps"][0];
    assert_eq!(first["id"], "PS-DIS-002");
    assert_eq!(first["tier"], 1);
    assert!(!everything(&h.call(FPHS, json!({"statuses": [{"standard_id": "PS-DIS-002", "status": "not_met", "note": "Private note for the file"}], "write_documents": false})).await).contains("Private note"));
    // The note is in the workbook only.
    let files = d["files"]["files"].as_array().unwrap();
    let xlsx = files
        .iter()
        .map(|f| f.as_str().unwrap())
        .find(|f| f.ends_with(".xlsx"))
        .unwrap();
    let tables = phn_local::table::read_tables(&h.data().join(xlsx)).unwrap();
    assert_eq!(tables[0].name, "Self-assessment");
    let all: String = tables
        .iter()
        .flat_map(|t| t.rows.iter().flatten().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(all.contains("Private note for the file"));

    // A worksheet that has contact people beside the statuses: only the ID and status columns
    // (and delivered by, priority, notes) are read.
    let s = call_ok(
        &h,
        FPHS,
        json!({"assessment_path": "fphs_assessment.csv", "write_documents": false}),
    )
    .await;
    let f = &s["data"]["assessment_file"];
    assert_eq!(f["rows_with_status"], 6, "{f}");
    assert_eq!(s["data"]["total_gaps"], 4);
    let found = leaks(&everything(&h.call(FPHS, json!({"assessment_path": "fphs_assessment.csv"})).await), "data/fphs_assessment.csv", None);
    assert!(found.is_empty(), "{found:?}");
    // The delivered-by choices were read.
    assert!(s["data"]["delivered_by"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn fphs_problems_are_tool_errors() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(
        &h,
        FPHS,
        json!({"statuses": [{"standard_id": "XX-1", "status": "met"}]}),
    )
    .await;
    assert!(e.contains("not a standard ID"), "{e}");
    let e = call_err(
        &h,
        FPHS,
        json!({"statuses": [{"standard_id": "PS-ADM-001", "status": "perhaps"}]}),
    )
    .await;
    assert!(e.contains("unrecognized status"), "{e}");
    let e = call_err(&h, FPHS, json!({"town": "Springfeild"})).await;
    assert!(e.contains("Springfield"), "{e}");
    let e = call_err(&h, FPHS, json!({"assessment_path": "line_list.csv"})).await;
    assert!(e.contains("self-assessment worksheet"), "{e}");
    let e = call_err(&h, FPHS, json!({"assessment_path": "../etc/passwd"})).await;
    assert!(!e.is_empty());
}

fn write_policies(h: &common::Harness) {
    std::fs::write(
        h.data().join("policies/community_health_assessment_2024.md"),
        "# Community health assessment 2024\nThis community health assessment used a community survey, \
         focus group sessions, and key informant interviews.\n## Data sources\nVital statistics and a \
         data dashboard informed the assessment.\n",
    )
    .unwrap();
    std::fs::write(
        h.data().join("policies/strategic_plan.md"),
        "# Strategic plan 2025-2029\nThe mission and vision guide our strategic priorities.\n",
    )
    .unwrap();
}

#[tokio::test]
async fn phab_crosswalk_finds_evidence_and_gaps_and_writes_files() {
    let h = start(|c| c.offline = true).await;
    write_policies(&h);
    let s = call_ok(&h, PHAB, json!({"folder": "policies"})).await;
    let d = &s["data"];
    assert_eq!(d["phab_version"], "Version 2022");
    let standards = d["standards"].as_array().unwrap();
    assert_eq!(standards.len(), 20);
    let s11 = standards.iter().find(|x| x["id"] == "1.1").unwrap();
    assert_eq!(s11["evidence"], "strong");
    assert_eq!(
        s11["documents"][0]["file"],
        "community_health_assessment_2024.md"
    );
    assert!(
        d["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g == "2.2")
    );
    for st in standards {
        for doc in st["documents"].as_array().unwrap() {
            if let Some(p) = doc["passage"].as_str() {
                assert!(p.split_whitespace().count() <= 20, "{p}");
            }
        }
    }
    assert!(pending(&s));
    let c = caveats(&s);
    assert!(c.contains("copyrighted") && c.contains("could not"), "{c}");
    assert!(d["license_note"].as_str().unwrap().contains("copyrighted"));
    // Local source mode and the file names only.
    assert!(s["sources"].as_array().unwrap().iter().any(|x| x["mode"] == "local"));
    let files: Vec<String> = d["files"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap().to_string())
        .collect();
    assert_eq!(files.len(), 2);
    let docx = files.iter().find(|f| f.ends_with(".docx")).unwrap();
    let text = docx_text(&h, docx);
    assert!(text.contains("copyrighted") && text.contains("Gaps"), "{text}");
    let xlsx = files.iter().find(|f| f.ends_with(".xlsx")).unwrap();
    let wb = phn_local::table::read_tables(&h.data().join(xlsx)).unwrap();
    assert_eq!(wb[0].name, "Crosswalk");
}

#[tokio::test]
async fn phab_crosswalk_leaves_out_client_files_and_transcripts() {
    let h = start(|c| c.offline = true).await;
    write_policies(&h);
    // The whole data folder holds client notes, 51A notes, a client letter, and two transcripts.
    let r = h.call(PHAB, json!({"folder": "."})).await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    let s = r.structured_content.clone().unwrap();
    let idx = &s["data"]["index"];
    assert!(idx["left_out_client_like"].as_u64().unwrap() >= 3, "{idx}");
    assert!(idx["left_out_transcripts"].as_u64().unwrap() >= 2, "{idx}");
    let all = everything(&r);
    for file in [
        "data/client_note.txt",
        "data/51a_notes.txt",
        "data/client_letter.html",
        "data/focus_group_transcript.txt",
        "data/key_informant_interview.txt",
        "data/study_abstract.txt",
    ] {
        let found = leaks(&all, file, None);
        assert!(found.is_empty(), "{file}: {found:?}");
    }
    // Their file names are not listed in the response either.
    for name in ["client_note.txt", "51a_notes.txt", "focus_group_transcript.txt"] {
        assert!(!all.contains(name), "{name} named in the response");
    }
}

#[tokio::test]
async fn phab_crosswalk_maps_measures_from_the_nurses_own_list() {
    let h = start(|c| c.offline = true).await;
    write_policies(&h);
    std::fs::write(
        h.data().join("phab_measures.csv"),
        "Measure ID,Short title,Keywords\n1.1.1,Zebra assessment cycle,community health assessment; community survey\n10.1.1,Planning cycle,strategic plan\n2.2.1,Drills,tabletop exercise\n",
    )
    .unwrap();
    let s = call_ok(
        &h,
        PHAB,
        json!({"folder": "policies", "measures_path": "phab_measures.csv"}),
    )
    .await;
    let m = &s["data"]["measures"];
    assert_eq!(m["measures"], 3);
    assert_eq!(m["results"][0]["standard"], "1.1");
    assert_eq!(m["results"][0]["evidence"], "strong");
    assert_eq!(m["results"][2]["evidence"], "none");
    // Titles are in the files only.
    assert!(!everything(&h.call(PHAB, json!({"folder": "policies", "measures_path": "phab_measures.csv", "write_documents": false})).await).contains("Zebra"));
    let files = s["data"]["files"]["files"].as_array().unwrap();
    let xlsx = files
        .iter()
        .map(|f| f.as_str().unwrap())
        .find(|f| f.ends_with(".xlsx"))
        .unwrap();
    let wb = phn_local::table::read_tables(&h.data().join(xlsx)).unwrap();
    assert!(wb.iter().any(|t| t.name == "Measures"));
    // A measures list that is a roster is refused.
    let e = call_err(
        &h,
        PHAB,
        json!({"folder": "policies", "measures_path": "staff_roster.csv"}),
    )
    .await;
    assert!(e.contains("Measure ID"), "{e}");
}

#[tokio::test]
async fn phab_crosswalk_problems_are_tool_errors() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(&h, PHAB, json!({"folder": "resources"})).await;
    assert!(e.contains("no usable documents"), "{e}");
    let e = call_err(&h, PHAB, json!({"folder": "policies", "domains": [11]})).await;
    assert!(e.contains("1 to 10"), "{e}");
    let e = call_err(&h, PHAB, json!({"folder": "no_such_folder"})).await;
    assert!(!e.is_empty());
}

fn plan_args() -> Value {
    json!({
        "department": "Example Health Department",
        "period_start": 2027,
        "period_end": 2031,
        "mission": "Protect and promote the health of everyone in our towns.",
        "swot": {"strengths": ["Trusted public health nurses"], "weaknesses": ["No epidemiologist"],
                 "opportunities": ["Shared service funding"], "threats": ["Grant funding ends in 2028"]},
        "stakeholders": [
            {"name": "Board of health", "power": 5, "interest": 5},
            {"name": "Select board", "power": 5, "interest": 2},
            {"name": "Parent groups", "power": 2, "interest": 4},
            {"name": "Regional planning agency", "power": 1, "interest": 1}],
        "priorities": [{
            "title": "Strengthen disease surveillance",
            "chip_priority": "Behavioral health",
            "goals": [{"statement": "Have epidemiology support",
                       "objectives": [{"statement": "Contract epidemiology support", "measure": "Contract signed",
                                       "target": "1", "due": "2028-06"}],
                       "strategies": [{"statement": "Join the district's epidemiology service",
                                       "responsible": "Health director", "timeline": "2027"}]}]}],
        "chip_title": "Community Health Improvement Plan 2026",
        "chip_priorities": ["Behavioral health", "Housing"]
    })
}

#[tokio::test]
async fn strategic_plan_builds_the_scaffold_grid_and_draft() {
    let h = start(|c| c.offline = true).await;
    let mut args = plan_args();
    args["include_svg"] = json!(true);
    let s = call_ok(&h, PLAN, args).await;
    let d = &s["data"];
    assert_eq!(d["analysis"]["method"], "swot");
    assert_eq!(d["analysis"]["entries_total"], 4);
    assert_eq!(d["analysis"]["strategy_prompts"].as_array().unwrap().len(), 4);
    let q: Vec<(String, String)> = d["stakeholders"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["name"].as_str().unwrap().to_string(),
                r["quadrant"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(q[0].1, "Manage closely");
    assert_eq!(q[1].1, "Keep satisfied");
    assert_eq!(q[2].1, "Keep informed");
    assert_eq!(q[3].1, "Monitor");
    let svg = d["stakeholders"]["svg"].as_str().unwrap();
    assert!(svg.contains("role=\"img\"") && svg.contains("<title") && svg.contains("<desc"));
    assert!(!svg.contains("<script"));
    assert_eq!(d["priorities"][0]["goals"][0]["objectives"][0]["code"], "O1.1.1");
    // CHIP: Housing has nothing behind it.
    assert_eq!(d["chip"]["rows"][0]["linked_strategic_priorities"].as_array().unwrap().len(), 1);
    let ids: Vec<&str> = d["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"chip_unlinked"), "{ids:?}");
    assert!(ids.contains(&"stakeholder_gap"), "{ids:?}");
    // PHAB requirements are unverified and the tool says so.
    assert_eq!(d["phab"]["status"], "unverified");
    let c = caveats(&s);
    assert!(c.contains("unverified"), "{c}");
    assert!(!summary(&h.call(PLAN, plan_args()).await).to_lowercase().contains("meets phab"));
    assert!(pending(&s));
    // Files: the draft plan and the grid as SVG and PNG.
    let f = &d["files"];
    let doc = docx_text(&h, f["document"].as_str().unwrap());
    assert!(doc.contains("Example Health Department") && doc.contains("[to be completed]"), "{doc}");
    assert!(doc.contains("O1.1.1"));
    for k in ["svg", "png"] {
        assert!(h.data().join(f[k].as_str().unwrap()).exists());
    }
}

#[tokio::test]
async fn strategic_plan_problems_are_tool_errors() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(&h, PLAN, json!({"period_start": 1800})).await;
    assert!(e.contains("1900-2100"), "{e}");
    let e = call_err(
        &h,
        PLAN,
        json!({"stakeholders": [{"name": "X", "power": 7, "interest": 1}]}),
    )
    .await;
    assert!(e.contains("1 to 5"), "{e}");
    let e = call_err(&h, PLAN, json!({"swot": {}, "soar": {}})).await;
    assert!(e.contains("choose one"), "{e}");
    let e = call_err(
        &h,
        PLAN,
        json!({"priorities": [{"title": "P", "goals": [{"statement": "G",
               "objectives": [{"statement": "O", "due": "soon"}]}]}]}),
    )
    .await;
    assert!(e.contains("due date"), "{e}");
}

#[tokio::test]
async fn contact_details_typed_into_the_plan_are_reported() {
    let h = start(|c| c.offline = true).await;
    let r = h
        .call(
            PLAN,
            json!({"stakeholders": [{"name": "Coalition", "power": 3, "interest": 4,
                                     "owner": "Call 413-555-0199"}],
                   "write_document": false}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    let s = r.structured_content.unwrap();
    assert!(!s["privacy"]["identifiers_detected"].as_array().unwrap().is_empty());
}
