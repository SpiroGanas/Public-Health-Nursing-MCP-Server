//! Phase 13 tools (emergency preparedness II): EPR-07 continuity_of_operations_planner (COOP).

use phn_core::Envelope;
use phn_tools::epr::{CoopArgs, CoopResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

#[allow(unused_imports)]
use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase13_coop_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "continuity_of_operations_planner",
        description = "Draft a continuity of operations (COOP) plan for a Massachusetts board of \
            health or health department, organized by FEMA's ten continuity elements: essential \
            functions in priority tiers with recovery time objectives (seeded with board of \
            health functions and their authority: 24/7 disease reporting under 105 CMR 300.000, \
            isolation and quarantine, nuisances, housing and food emergencies, burial permits, \
            Title 5, dispensing, shelters), orders of succession, delegations of authority \
            (including the board's emergency agent under M.G.L. c. 111 section 30), continuity \
            facilities, communications, essential records, human capital with a pandemic \
            absenteeism check (40% planning assumption), a dated test/training/exercise \
            schedule, devolution, reconstitution, and a pandemic annex. Returns which elements \
            are provided, drafted, or missing. Writes a Word plan and Excel workbook to \
            /data/output. Staff names and positions may be given; phone numbers and emails come \
            only from a roster file in /data (roster_path) and appear only in the files.",
        annotations(
            title = "Continuity of operations (COOP) planner",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<CoopResult>>()
    )]
    async fn continuity_of_operations_planner(
        &self,
        Parameters(args): Parameters<CoopArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epr::continuity_of_operations_planner(ctx, args)
        })
        .await
    }
}
