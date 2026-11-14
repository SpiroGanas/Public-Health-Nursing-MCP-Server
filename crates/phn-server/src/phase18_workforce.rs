//! Phase 18 tools (Workforce, board of health, standing orders): LEAD-06, LEAD-07, LEAD-08.

use phn_core::Envelope;
use phn_tools::leadership::{
    BoardMeetingArgs, BoardMeetingResult, StandingOrderArgs, StandingOrderResult, WorkforceArgs,
    WorkforceResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking};

// Phase 18 workforce: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase18_workforce_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "assess_workforce_competencies",
        description = "Self-assessment of your team against the eight domains and three tiers of \
            the Council on Linkages Core Competencies for Public Health Professionals (2021) or \
            the Community/Public Health Nursing Competencies (Quad Council, 2018), with a team \
            training plan and a tracker for the Massachusetts local public health workforce \
            standards (credentials by position, from DPH's Performance Standards). With no files it \
            writes a blank self-assessment workbook (XLSX, live formulas) and guide to \
            /data/output. Give the filled ratings file as ratings_path (staff code, tier, domain, \
            rating 1 to 4) and optionally credentials_path (staff code, position, hire date, \
            credentials completed): it returns team means and gaps to a target (default 3, a tool \
            default), a prioritized training plan, and credential status (complete, due soon, \
            overdue) by position, and writes the workbook and a DOCX plan. Privacy: staff are \
            identified by a code, never a name (name columns are refused); no result is shown for \
            fewer than 3 people (a team of fewer than 3 gets one total at most); individual rows stay \
            in the workbook. The frameworks' numbered competency statements could not be read, so \
            ratings are by domain. Decision support, not a personnel evaluation or a compliance \
            finding. Content is a draft pending expert review.",
        annotations(
            title = "Workforce competency assessment and credential tracker",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<WorkforceResult>>()
    )]
    async fn assess_workforce_competencies(
        &self,
        Parameters(args): Parameters<WorkforceArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::leadership::assess_workforce_competencies(ctx, args)
        })
        .await
    }

    #[tool(
        name = "board_of_health_meeting_assistant",
        description = "Helps a board of health run Open Meeting Law-compliant meetings \
            (M.G.L. c. 30A, ss. 18-25; 940 CMR 29.00). Actions: notice_check calculates the latest \
            time to post the meeting notice (48 hours ahead, leaving out Saturdays, Sundays, and \
            Massachusetts legal holidays 2026-2028; Suffolk county bodies also skip Evacuation Day \
            and Bunker Hill Day), checks a posting time, the notice contents, topic specificity, \
            remote access, and executive session items; agenda writes the agenda and notice draft \
            (DOCX) with the post-by time and the chair's checklist and executive session script on \
            staff pages; staff_report writes a staff report template (DOCX) filled with the sections \
            you give; minutes drafts minutes (DOCX) from your notes file or text (lines such as \
            Present:, Item:, Motion:, Vote:, Exhibit:), checks the parts the law requires (date, \
            time, place, members, discussion summary, documents, votes by roll call for remote or \
            executive session), and scans the notes for identifiers (line numbers only); the notes \
            are never returned. oml_guide returns the requirements with sources (ten executive \
            session purposes, minutes contents and approval timing). Decision support, not legal \
            advice; check town counsel or the Attorney General's Division of Open Government. \
            Content is a draft pending expert review.",
        annotations(
            title = "Board of health meeting assistant (Open Meeting Law)",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<BoardMeetingResult>>()
    )]
    async fn board_of_health_meeting_assistant(
        &self,
        Parameters(args): Parameters<BoardMeetingArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::leadership::board_of_health_meeting_assistant(ctx, args)
        })
        .await
    }

    #[tool(
        name = "draft_standing_orders",
        description = "Drafts a standing order or protocol (vaccine, other medication, or nursing \
            activity) for your medical director to review and sign, as a Word file in /data/output. \
            The structure follows the Massachusetts Board of Registration in Nursing's Advisory \
            Ruling 9324 (the ten elements every standing order needs; what it cannot allow) and, for \
            vaccines, Advisory Ruling 9804 (VIS, education and consent, MIIS reporting, \
            documentation). Sections you write (with the source you cite for each clinical section) \
            go in; everything else is a [NURSE: ...] or [MEDICAL DIRECTOR: ...] placeholder, with \
            signature lines and review dates. NO CLINICAL CONTENT is bundled or invented: doses, \
            routes, inclusion and exclusion criteria, contraindications, and precautions come only \
            from your cited source. Returns which sections are drafted, coverage of each required \
            element, wording cues that may conflict with the limits (menu of choices, changing the \
            order, prescribing), and, for vaccines, DPH's signed 2026-2027 standing orders for local \
            boards of health to check first. Typed text that looks like an identifier is refused. \
            Clinical decision support only; the medical director signs. Content is a draft pending \
            expert review.",
        annotations(
            title = "Standing order drafting",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<StandingOrderResult>>()
    )]
    async fn draft_standing_orders(
        &self,
        Parameters(args): Parameters<StandingOrderArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::leadership::draft_standing_orders(ctx, args)
        })
        .await
    }
}
