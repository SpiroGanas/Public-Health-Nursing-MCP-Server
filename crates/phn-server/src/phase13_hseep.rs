//! Phase 13 tools (emergency preparedness II): EPR-08 design_hseep_exercise and EPR-09 build_after_action_report (HSEEP).

use phn_core::Envelope;
use phn_tools::epr::{AarArgs, AarResult, ExerciseArgs, ExerciseResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

#[allow(unused_imports)]
use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase13_hseep_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "design_hseep_exercise",
        description = "Design an HSEEP-consistent exercise (tabletop by default; also seminar, \
            workshop, game, drill, functional, or full-scale) for a Massachusetts local health \
            department. Scenarios: fully developed tabletops for a measles outbreak, a coastal \
            storm with power outage, and extreme heat (modules, injects, key issues, discussion \
            questions by functional group), seeds for mass dispensing, pandemic influenza, \
            cyberattack, water contamination, mass casualty, and hazmat, or your own. Checks \
            that objectives are SMART and maps each to CDC PHEP capabilities and FEMA core \
            capabilities; gives the schedule and a planning timeline (C&O, IPM, MPM, FPM, \
            after-action dates). Writes the Situation Manual and Facilitator Guide (or Exercise \
            Plan and MSEL), Exercise Evaluation Guides, a Participant Feedback Form, and a \
            sign-in sheet to /data/output. Staff phone numbers and emails come only from \
            roster_path and appear only in those files.",
        annotations(
            title = "Design an HSEEP exercise",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<ExerciseResult>>()
    )]
    async fn design_hseep_exercise(
        &self,
        Parameters(args): Parameters<ExerciseArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epr::design_hseep_exercise(ctx, args)
        })
        .await
    }

    #[tool(
        name = "build_after_action_report",
        description = "Build an HSEEP After-Action Report and Improvement Plan (AAR/IP) for an \
            exercise or a real incident from structured hotwash and evaluator findings: \
            objectives with PHEP and FEMA core capabilities and P/S/M/U ratings, strengths and \
            areas for improvement (with reference, analysis, and root cause), and corrective \
            actions (capability element, responsible organization, point of contact, start and \
            completion dates). Checks that every area for improvement has a complete SMART \
            corrective action, that areas for improvement state the gap rather than the fix, and \
            that the text names no people or client identifiers. Totals Participant Feedback \
            Form ratings from a CSV/XLSX in /data (aggregates only, small counts hidden, comments \
            never read back). Writes the AAR (DOCX/PDF) and an XLSX improvement plan tracker to \
            /data/output. Share de-identified notes only.",
        annotations(
            title = "Build an after-action report and improvement plan",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<AarResult>>()
    )]
    async fn build_after_action_report(
        &self,
        Parameters(args): Parameters<AarArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epr::build_after_action_report(ctx, args)
        })
        .await
    }
}
