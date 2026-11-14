//! Phase 15 tools (Community health assessment): EQ-08, EQ-09, EQ-10.

use crate::server::{PhnServer, blocking, respond};
use phn_core::Envelope;
use phn_tools::equity::{
    CommunityAssessmentArgs, CommunityAssessmentResult, PrioritizeArgs, PrioritizeResult,
    QualitativeArgs, QualitativeResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

// Phase 15 cha: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase15_cha_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "community_health_assessment_builder",
        description = "Guides a community health assessment with MAPP 2.0 (NACCHO) for a \
            Massachusetts town or county (default: your saved town) and drafts report sections \
            as a DOCX in /data/output. Lays out MAPP 2.0's three phases and three assessments \
            (MAPP 2.0 was checked against journal articles, not NACCHO's own pages, and the \
            result says so). Community status assessment: assembles the community profile with \
            CDC PLACES chronic disease estimates and the Social Vulnerability Index, language \
            access, environmental justice populations, climate and health vulnerability, food \
            access, drinking water quality, and fluoridation, each from this server's own tools \
            (live, then cache, then snapshot; a section that fails is reported and the rest \
            are still returned). Community partner assessment: a sector checklist, and counts \
            of your partner roster file by sector (organizations only; contact people are not \
            read). Community context assessment: prompts for focus groups and interviews, \
            with analyze_qualitative_data for the transcripts. Hospital CHNAs: lists hospitals \
            in and near the area from CMS data, the federal CHNA rule (26 CFR 1.501(r)-3) and \
            the Massachusetts Attorney General's Community Benefits Guidelines, links to find \
            each hospital's CHNA (the Attorney General's reports site), and, for CHNA reports \
            you save in /data (PDF, DOCX, HTML, or TXT), the priority lists with page \
            citations and the themes they share. Decision support; it does not replace your \
            partners' and residents' input. Can take a minute when data is fetched live.",
        annotations(
            title = "Community health assessment builder",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<CommunityAssessmentResult>>()
    )]
    async fn community_health_assessment_builder(
        &self,
        Parameters(args): Parameters<CommunityAssessmentArgs>,
    ) -> CallToolResult {
        let gathered = match phn_tools::equity::cha::gather(&self.ctx, &args).await {
            Ok(g) => g,
            Err(e) => return respond::<CommunityAssessmentResult>(Err(e)),
        };
        blocking(&self.ctx, move |ctx| {
            phn_tools::equity::cha::assemble(ctx, args, gathered)
        })
        .await
    }

    #[tool(
        name = "prioritize_health_issues",
        description = "Ranks community health issues you give it, and documents how. Hanlon \
            method: the basic priority rating BPR = ((A + B) x C / 3) x D, where A is the size \
            of the problem (0-10), B its seriousness (0-20, or four factors of 0-5: urgency, \
            severity, economic loss, effect on others), C the effectiveness of an intervention \
            (0-10), and D the PEARL test (propriety, economics, acceptability, resources, \
            legality: 1 only if all five are yes). PEARL can multiply the score (default), \
            screen issues out first, or be turned off. Also places each issue on a strategy \
            grid (importance by feasibility; this grid is the project's own) and scores a \
            nominal group vote from totals (how many participants gave each issue each rank; \
            never names). Ties share a rank and are listed, not broken; missing ratings are \
            listed as still needed, not scored as zero; close calls are flagged; the two \
            methods' ranks are shown side by side, not blended. The formula comes from the \
            peer-reviewed literature (NACCHO's own worksheet could not be read) and every \
            rating is your judgment: record the rationale. Writes the method, ratings, and \
            rationale to a DOCX or PDF in /data/output. Reads no files; type community issues, \
            not people.",
        annotations(
            title = "Prioritize health issues (Hanlon, PEARL, nominal group)",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<PrioritizeResult>>()
    )]
    async fn prioritize_health_issues(
        &self,
        Parameters(args): Parameters<PrioritizeArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::equity::prioritize_health_issues(ctx, args)
        })
        .await
    }

    #[tool(
        name = "analyze_qualitative_data",
        description = "Helps analyze focus group and key informant interview transcripts saved \
            in /data (TXT, MD, DOCX, text PDF, HTML). The transcripts are read only on this \
            machine and nothing from them is returned except counts: names, organizations, \
            places, phone numbers, email and street addresses, dates, and record numbers are \
            removed (speakers become P1, P2, ...; an optional names file in /data adds the \
            names to look for), then the text is coded with keywords from your codebook (a CSV, \
            XLSX, or YAML file with code, definition, keywords, theme) or, if you give none, a \
            draft starter codebook. The response has the scrub report (counts), the codebook, \
            and for each theme how many different participants raised it and in how many \
            transcripts; a theme raised by fewer than three different participants is withheld \
            entirely. No transcript sentence, speaker label, or file name is ever returned. The \
            coded excerpt matrix (de-identified excerpts under each code, a transcript key, and \
            the codebook) and a theme summary go to /data/output only. Keyword coding finds \
            words, not meaning: read the excerpts and recode by hand before reporting a \
            finding.",
        annotations(
            title = "Analyze qualitative data (focus groups and interviews)",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<QualitativeResult>>()
    )]
    async fn analyze_qualitative_data(
        &self,
        Parameters(args): Parameters<QualitativeArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::equity::analyze_qualitative_data(ctx, args)
        })
        .await
    }
}
