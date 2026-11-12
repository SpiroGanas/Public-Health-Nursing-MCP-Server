//! Phase 18 tools (FPHS, PHAB, strategic planning): LEAD-03, LEAD-04, LEAD-05.

use phn_core::Envelope;
use phn_tools::leadership::{
    FphsArgs, FphsResult, PhabArgs, PhabResult, StrategicPlanArgs, StrategicPlanResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking};

// Phase 18 strategy: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase18_strategy_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "assess_foundational_public_health_services",
        description = "Gap analysis of your health department against the Massachusetts local \
            public health standards (G.L. c. 111, s. 27D, the 2020 State Action for Public Health \
            Excellence law) and the Foundational Public Health Services (FPHS) framework, with \
            shared-service options. Four sets of standards: DPH's Performance Standards (the \
            mandated local responsibilities, about 90, from DPH's workbook), DPH's workforce \
            standards by position and inspection type, the 56 FPHS Headline Responsibilities, and \
            the program terms in s. 27D(a). You record your own status for each (met, partial, \
            not_met, not_applicable, plus who delivers it and an optional priority): the tool \
            never invents an answer. With no statuses it writes a self-assessment worksheet (XLSX) \
            and guide (DOCX) to /data/output; give the filled worksheet back as assessment_path \
            (or type a few statuses) and it returns counts by set and area, a gap table, a priority \
            view in three tiers (legal mandates first; the rules are shown and you can override \
            any item), shared-service options (G.L. c. 40 s. 4A arrangements, districts, PHE grants, \
            the shared service arrangement DPH lists for your town and its training hub), and writes \
            the gap analysis (DOCX and XLSX) to /data/output. Decision support, not a compliance \
            finding; notes in your worksheet stay in the files. Content is a draft pending expert review.",
        annotations(
            title = "Foundational public health services gap analysis",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<FphsResult>>()
    )]
    async fn assess_foundational_public_health_services(
        &self,
        Parameters(args): Parameters<FphsArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::leadership::assess_foundational_public_health_services(ctx, args)
        })
        .await
    }

    #[tool(
        name = "phab_accreditation_crosswalk",
        description = "Maps the documents in a folder of your own (policies, plans, reports, \
            minutes: PDF, DOCX, Markdown, text, HTML) to the 10 domains and 20 standards of the \
            PHAB Standards & Measures Version 2022, using the same document index as \
            search_agency_policies. Shows each standard's evidence (strong, some, none found), the \
            documents behind it by your own file names, short matched passages of at most 20 words \
            with names and contact details redacted, and the gaps. Files that look like client \
            records or interview transcripts are left out and counted. PHAB's measure text is \
            copyrighted and could not be read, so measure IDs are not bundled: give measures_path \
            a list from your own copy of PHAB's document to crosswalk measures too. Writes the \
            crosswalk (DOCX and XLSX) to /data/output. A keyword match shows a document mentions \
            a topic, not that it meets a measure; check PHAB's documentation requirements.",
        annotations(
            title = "PHAB accreditation document crosswalk",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<PhabResult>>()
    )]
    async fn phab_accreditation_crosswalk(
        &self,
        Parameters(args): Parameters<PhabArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::leadership::phab_accreditation_crosswalk(ctx, args)
        })
        .await
    }

    #[tool(
        name = "build_strategic_plan",
        description = "Scaffolding for a health department strategic plan from content you \
            supply (the tool adds no facts): a SWOT or SOAR analysis with prompts, a stakeholder \
            power and interest grid (accessible SVG and PNG plus a table, quadrants Manage closely, \
            Keep satisfied, Keep informed, Monitor), strategic priorities with goals, objectives \
            (measure, baseline, target, due date), and strategies (responsible, timeline), coded \
            P1, G1.1, O1.1.1, S1.1.1, and the link to your community health improvement plan \
            (CHIP) priorities. Checks for blanks (an objective with no measure, target, or date; \
            a goal with no strategy; a CHIP priority nothing supports; a high power, high interest \
            stakeholder in no strategy) and writes a draft plan (DOCX) and the grid to \
            /data/output. PHAB's strategic plan requirements could not be verified, so the tool \
            says so and never claims a plan meets them; it lists what to check in PHAB's document.",
        annotations(
            title = "Strategic plan builder",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<StrategicPlanResult>>()
    )]
    async fn build_strategic_plan(
        &self,
        Parameters(args): Parameters<StrategicPlanArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::leadership::build_strategic_plan(ctx, args)
        })
        .await
    }
}
