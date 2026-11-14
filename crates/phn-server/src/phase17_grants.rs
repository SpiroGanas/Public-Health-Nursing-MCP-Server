//! Phase 17 tools (Grant search, NOFO analysis, narratives): PLAN-06, PLAN-07, PLAN-08.

use phn_core::Envelope;
use phn_tools::planning::{
    AnalyzeAnnouncementArgs, AnnouncementAnalysis, GrantSearchArgs, GrantSearchResult,
    NarrativeArgs, NarrativeResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

// Phase 17 grants: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase17_grants_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "search_grant_opportunities",
        description = "Finds funding opportunities a local or state health agency can act on. \
            Federal: searches the Simpler.Grants.gov API by topic, who may apply (local health \
            agency, state agency, tribal health, or any), closing-date window, and award size \
            when you have set PHN_GRANTS_GOV_API_KEY; without a key, or when the request fails, \
            it says it is in snapshot mode, and the snapshot holds no federal listings because \
            Grants.gov could not be reached when the server was built. Massachusetts: the \
            Operational Services Division's weekly list of new COMMBUYS bids (read live from \
            mass.gov when possible, otherwise the bundled copy with its date), tagged by topic \
            and by whether the title looks like a grant, a request for response, or a purchase; \
            the list gives no amount, eligibility, or description, so each is flagged to read on \
            COMMBUYS. Also returns where else to look (CDC, EPA, DPH requests for response, the \
            COMMBUYS vendor notices) and Massachusetts procurement facts from DPH and OSD pages. \
            Only topic words and filters leave the server; a topic that looks like a personal \
            identifier is refused. A match is a lead: read the full announcement before relying \
            on it. Content is pending review.",
        annotations(
            title = "Search grant and bid opportunities",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<GrantSearchResult>>()
    )]
    async fn search_grant_opportunities(
        &self,
        Parameters(args): Parameters<GrantSearchArgs>,
    ) -> CallToolResult {
        respond(phn_tools::planning::search_grant_opportunities(&self.ctx, args).await)
    }

    #[tool(
        name = "analyze_funding_announcement",
        description = "Reads a notice of funding opportunity (NOFO) or a Massachusetts request \
            for response (RFR) that you saved in /data (PDF, DOCX, HTML, TXT, or Markdown) and \
            turns it into a compliance checklist: eligibility passages and the applicant types \
            they name, every deadline (submission, questions, conference, letter of intent, \
            award, start) with days from today, page, word, and character limits and format \
            rules, required attachments and named forms, scoring criteria with points (or the \
            rating words and numbered components an RFR uses) mapped to the narrative sections \
            that answer them, dollar amounts, the project period, how to submit, and a response \
            outline in the order the funder scores. It uses pattern matching, not judgment, and \
            says exactly what it could not find. Returns structure, counts, dates, and short \
            labels only, never long passages or the contact people named in the announcement; \
            writes the checklist DOCX and XLSX (with a 'Scoring criteria' sheet that \
            draft_grant_narrative can read) to /data/output. Tested on synthetic notices and \
            public Massachusetts announcements, not on a real federal NOFO.",
        annotations(
            title = "Analyze a funding announcement",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<AnnouncementAnalysis>>()
    )]
    async fn analyze_funding_announcement(
        &self,
        Parameters(args): Parameters<AnalyzeAnnouncementArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::planning::analyze_funding_announcement(ctx, args)
        })
        .await
    }

    #[tool(
        name = "draft_grant_narrative",
        description = "Drafts a needs statement, project narrative, and sustainability section \
            from templates and the facts you supply, organized by the funder's scoring criteria \
            (typed in, or read from the checklist that analyze_funding_announcement wrote). \
            Statistics come only from the data_points you pass, each shown with its geography, \
            source, and year; program facts come only from the facts you pass; anything missing \
            becomes a [NURSE: ...] placeholder. The tool never invents a statistic, a program \
            detail, or a citation, and refuses client identifiers. Each criterion is mapped to \
            the section that answers it, with writing prompts; budget and form criteria are \
            listed as not drafted here. Returns the draft text and counts of filled parts and \
            placeholders, and writes a DOCX to /data/output. Always check the draft against the \
            announcement's page limit, format rules, and criteria.",
        annotations(
            title = "Draft a grant narrative",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<NarrativeResult>>()
    )]
    async fn draft_grant_narrative(
        &self,
        Parameters(args): Parameters<NarrativeArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::planning::draft_grant_narrative(ctx, args)
        })
        .await
    }
}
