//! Phase 13 tools (emergency preparedness II): EPR-01 generate_ics_forms (ICS forms and Incident Action Plan).

use phn_core::Envelope;
use phn_tools::epr::{IcsArgs, IcsResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

#[allow(unused_imports)]
use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase13_ics_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "generate_ics_forms",
        description = "Fill FEMA Incident Command System forms for a health department incident \
            and assemble an Incident Action Plan (Word, PDF, or both, in /data/output): ICS 201 \
            Incident Briefing, 202 Incident Objectives, 203 Organization Assignment List, 204 \
            Assignment List (one per Division/Group), 205 Radio Communications Plan, 205A \
            Communications List, 206 Medical Plan, 207 Organization Chart, 208 Safety Message, \
            213 General Message, 213RR Resource Request, 214 Activity Log, and 215A Safety \
            Analysis, with FEMA's block numbers and labels. Default: the IAP set (202-208). \
            Give incident name, operational period, objectives, positions and names, \
            assignments, radio channels, and the medical plan; staff phone numbers and emails \
            come only from a roster file in /data (roster_path) and never appear in chat. \
            Returns which required blocks are still blank, an IAP checklist, and suggested SMART \
            objectives and safety analysis for measles, respiratory outbreak, mass dispensing, \
            coastal storm, heat, flood, or hazmat incidents. Optional blank forms.",
        annotations(
            title = "Generate ICS forms and Incident Action Plan",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<IcsResult>>()
    )]
    async fn generate_ics_forms(&self, Parameters(args): Parameters<IcsArgs>) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epr::generate_ics_forms(ctx, args)
        })
        .await
    }
}
